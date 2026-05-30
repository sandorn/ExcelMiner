import { useState, useEffect, useRef, useCallback } from 'react';
import {
    Card,
    Button,
    Input,
    Space,
    DatePicker,
    Tag,
    message,
    Row,
    Col,
    Typography,
} from 'antd';
import {
    PlayCircleOutlined,
    ThunderboltOutlined,
    ReloadOutlined,
    FolderOpenOutlined,
    KeyOutlined,
    LoadingOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import dayjs from 'dayjs';
import { useAppStore } from '../stores/appStore';
import { useAggregation } from '../hooks/useAggregation';
import { useAnalysis } from '../hooks/useAnalysis';
import { timestamp } from '../utils/format';
import type { Project, AppError } from '../types';

const { Text } = Typography;

/** 将 invoke 异常翻译为用户可读的错误码 */
function translateError(e: any): AppError {
    const msg = String(e);
    if (msg.includes('被 Excel 打开') || msg.includes('Device or resource busy')) {
        return { code: 'FILE_LOCKED', message: '文件被 Excel 占用，请关闭 Excel 后重试' };
    }
    if (msg.includes('API Key') || msg.includes('api_key')) {
        return { code: 'API_KEY', message: 'API Key 未配置或无效' };
    }
    if (msg.includes('超时') || msg.includes('timeout') || msg.includes('timed out')) {
        return { code: 'API_TIMEOUT', message: 'DeepSeek API 超时，请检查网络或重试' };
    }
    if (msg.includes('connect') || msg.includes('DNS') || msg.includes('refused')) {
        return { code: 'NETWORK', message: '网络连接失败，请检查网络' };
    }
    return { code: 'UNKNOWN', message: msg };
}

export default function MainPage() {
    const project = useAppStore((s) => s.project);
    const projectName = useAppStore((s) => s.projectName);
    const setProject = useAppStore((s) => s.setProject);
    const setLastError = useAppStore((s) => s.setLastError);

    // ── 自定义 Hooks ──
    const aggregation = useAggregation();
    const analysis = useAnalysis();

    // ── 配置状态 ──
    const [month, setMonth] = useState<dayjs.Dayjs>(dayjs());
    const [dataFolder, setDataFolder] = useState('');
    const [outputFile, setOutputFile] = useState('');
    const [model, setModel] = useState('deepseek-chat');
    const [apiKey, setApiKey] = useState('');
    const [apiKeyConfigured, setApiKeyConfigured] = useState(false);

    // ── 执行状态 ──
    const [running, setRunning] = useState(false);
    const [runningLabel, setRunningLabel] = useState('');
    const [btnSummaryDisabled, setBtnSummaryDisabled] = useState(false);
    const [btnSectorDisabled, setBtnSectorDisabled] = useState(false);
    const [btnCompanyDisabled, setBtnCompanyDisabled] = useState(false);

    // ── 阶段完成标记 ──
    const [phase1Done, setPhase1Done] = useState(false);
    const [phase2Done, setPhase2Done] = useState(false);
    const [phase3Done, setPhase3Done] = useState(false);
    const allPhasesDone = phase1Done && phase2Done && phase3Done;

    // ── 日志 ──
    const [logLines, setLogLines] = useState<string[]>([]);
    const logEndRef = useRef<HTMLDivElement>(null);

    const addLog = useCallback((msg: string) => {
        setLogLines((prev) => [...prev, `${timestamp()}  ${msg}`]);
    }, []);

    // 自动滚动日志到底部
    useEffect(() => {
        logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [logLines]);

    // ── 初始化: 读取 API Key ──
    useEffect(() => {
        invoke<string | null>('read_dskey', { section: 'EXCEL' })
            .then((key) => {
                if (key) {
                    setApiKey(key);
                    setApiKeyConfigured(true);
                }
            })
            .catch(() => {});
    }, []);

    // 加载默认配置
    useEffect(() => {
        invoke<{
            general: { language: string; theme: string; recent_projects: string[] };
            defaults: {
                default_data_folder: string;
                default_output_folder: string;
                api_url: string;
                model: string;
                system_prompt_path: string;
            };
        }>('get_default_config')
            .then((cfg) => {
                if (cfg.defaults.model) setModel(cfg.defaults.model);
            })
            .catch(() => {});
    }, []);

    // 首次加载项目时打印启动日志（仅一次）
    const projectLoadedRef = useRef(false);
    useEffect(() => {
        if (project && !projectLoadedRef.current) {
            projectLoadedRef.current = true;
            const m = dayjs(`${project.year}-${String(project.month).padStart(2, '0')}-01`);
            setMonth(m);
            setDataFolder(project.data_folder || '');
            setOutputFile(project.output_file || '');
            if (project.ai_config) {
                setModel(project.ai_config.model || 'deepseek-chat');
                if (project.ai_config.api_key) {
                    setApiKey(project.ai_config.api_key);
                    setApiKeyConfigured(true);
                }
            }
            addLog(`AardMiner 启动`);
            addLog(`数据源: ${project.data_folder}`);
            addLog(`结果文件: ${project.output_file}`);
            addLog(`公司数量: ${project.companies.length}`);
            addLog(`API Key: ${apiKeyConfigured ? '已配置' : '未配置'}`);
            addLog(`模型: ${project.ai_config?.model || 'deepseek-chat'}`);
            addLog('');
            addLog('点击按钮开始执行');
        } else if (project && projectLoadedRef.current) {
            // 后续 project 变更（保存配置）仅更新表单，不打印日志
            setDataFolder(project.data_folder || '');
            setOutputFile(project.output_file || '');
            if (project.ai_config) {
                setModel(project.ai_config.model || 'deepseek-chat');
            }
        }
    }, [project]);

    // ── 选择文件夹 ──
    const selectFolder = async (field: 'data' | 'output') => {
        try {
            const { open } = await import('@tauri-apps/plugin-dialog');
            const selected = await open({
                title: field === 'data' ? '选择数据源目录' : '选择结果文件',
                directory: field === 'data',
                multiple: false,
            });
            if (selected) {
                if (field === 'data') {
                    setDataFolder(selected as string);
                } else {
                    setOutputFile(selected as string);
                }
            }
        } catch (_) {}
    };

    // ── 保存配置 ──
    const saveConfig = async (): Promise<Project | null> => {
        if (!dataFolder) {
            message.warning('请选择数据源目录');
            return null;
        }
        if (!outputFile) {
            message.warning('请选择结果文件');
            return null;
        }

        const m = month.month() + 1; // dayjs 0-indexed
        const y = month.year();
        const name = `${y}年${m}月`;

        try {
            let p = project;
            if (!p) {
                // 创建新项目
                const sep = dataFolder.includes('\\') ? '\\' : '/';
                p = await invoke<Project>('create_project', {
                    name,
                    year: y,
                    month: m,
                    dataFolder,
                    outputFile: outputFile || `${dataFolder}${sep}【${name}】经营数据.xlsx`,
                });
            }

            const updated: Project = {
                ...p,
                year: y,
                month: m,
                data_folder: dataFolder,
                output_file: outputFile || p.output_file,
                ai_config: {
                    ...(p.ai_config || {
                        api_url: 'https://api.deepseek.com/v1/chat/completions',
                        api_key: '',
                        model: 'deepseek-chat',
                        temperature: 0.3,
                        max_tokens: 1500,
                        system_prompt_path: '',
                        batch_size: 3,
                        max_retries: 2,
                        quality_threshold: 8,
                    }),
                    model,
                    api_key: apiKey,
                },
            };

            await invoke('save_project', { project: updated });
            setProject(updated);
            return updated;
        } catch (e: any) {
            const err = translateError(e);
            setLastError(err);
            message.error(`保存配置失败: ${err.message}`, 5);
            return null;
        }
    };

    // ── 打开已有项目 ──
    const handleOpen = async () => {
        try {
            const { open } = await import('@tauri-apps/plugin-dialog');
            const selected = await open({
                title: '选择项目配置文件',
                filters: [{ name: '项目配置', extensions: ['toml'] }],
                multiple: false,
            });
            if (selected) {
                const p = await invoke<Project>('open_project', { path: selected as string });
                setProject(p);
                message.success(`项目 "${p.name}" 已打开`);
            }
        } catch (e: any) {
            const err = translateError(e);
            setLastError(err);
            message.error(`打开失败: ${err.message}`, 5);
        }
    };

    // ── 通用: 设置按钮状态并记录日志 ──
    const disableAll = (label: string) => {
        setRunning(true);
        setRunningLabel(label);
        setBtnSummaryDisabled(true);
        setBtnSectorDisabled(true);
        setBtnCompanyDisabled(true);
    };

    const enableAll = () => {
        setRunning(false);
        setRunningLabel('');
        setBtnSummaryDisabled(false);
        setBtnSectorDisabled(false);
        setBtnCompanyDisabled(false);
    };

    // ── 打开结果文件 ──
    const handleOpenResult = async () => {
        if (!outputFile) return;
        try {
            await invoke('open_in_explorer', { path: outputFile });
        } catch (_) {}
    };

    // ── 阶段1: 数据汇总 ──
    const handleSummary = async () => {
        enableAll();
        const p = await saveConfig();
        if (!p) return;
        setPhase1Done(false);
        disableAll('数据汇总');
        try {
            await aggregation.run(p, addLog);
            setPhase1Done(true);
        } catch (e: any) {
            const err = translateError(e);
            setLastError(err);
            message.error(err.message, 5);
        } finally {
            enableAll();
        }
    };

    // ── 阶段2: 板块AI分析 ──
    const handleSector = async () => {
        enableAll();
        const p = await saveConfig();
        if (!p) return;
        if (!apiKey) {
            addLog('错误: 未配置 API Key，跳过板块分析');
            return;
        }
        setPhase2Done(false);
        disableAll('业态分析');
        try {
            await analysis.runSegment(p, addLog);
            setPhase2Done(true);
        } catch (e: any) {
            const err = translateError(e);
            setLastError(err);
            message.error(err.message, 5);
        } finally {
            enableAll();
        }
    };

    // ── 阶段3: 公司AI分析 ──
    const handleCompany = async () => {
        enableAll();
        const p = await saveConfig();
        if (!p) return;
        if (!apiKey) {
            addLog('错误: 未配置 API Key，跳过公司分析');
            return;
        }
        setPhase3Done(false);
        disableAll('公司分析');
        try {
            await analysis.runCompany(p, addLog);
            setPhase3Done(true);
        } catch (e: any) {
            const err = translateError(e);
            setLastError(err);
            message.error(err.message, 5);
        } finally {
            enableAll();
        }
    };

    return (
        <div style={{ maxWidth: 700, margin: '0 auto' }}>
            {/* ======== 运行配置 ======== */}
            <Card size="small" style={{ marginBottom: 6 }}>
                <Row gutter={[6, 2]} align="middle">
                    <Col span={3}>
                        <Text type="secondary" style={{ fontSize: 12 }}>数据源:</Text>
                    </Col>
                    <Col span={17}>
                        <Input value={dataFolder} onChange={(e) => setDataFolder(e.target.value)} placeholder="选择数据源目录..." size="small" />
                    </Col>
                    <Col span={4}>
                        <Button size="small" icon={<FolderOpenOutlined />} onClick={() => selectFolder('data')} style={{ width: '100%' }} />
                    </Col>

                    <Col span={3}>
                        <Text type="secondary" style={{ fontSize: 12 }}>结果文件:</Text>
                    </Col>
                    <Col span={17}>
                        <Input value={outputFile} onChange={(e) => setOutputFile(e.target.value)} placeholder="选择 .xlsx ..." size="small" />
                    </Col>
                    <Col span={4}>
                        <Button size="small" icon={<FolderOpenOutlined />} onClick={() => selectFolder('output')} style={{ width: '100%' }} />
                    </Col>

                    <Col span={3}>
                        <Text type="secondary" style={{ fontSize: 12 }}>月份:</Text>
                    </Col>
                    <Col span={5}>
                        <DatePicker picker="month" value={month} onChange={(d) => d && setMonth(d)} format="M月" size="small" style={{ width: '100%' }} allowClear={false} />
                    </Col>
                    <Col span={2}>
                        <Text type="secondary" style={{ fontSize: 12 }}>年:</Text>
                    </Col>
                    <Col span={5}>
                        <DatePicker picker="year" value={month} onChange={(d) => d && setMonth(d)} format="YYYY年" size="small" style={{ width: '100%' }} allowClear={false} />
                    </Col>
                    <Col span={2}>
                        <Text type="secondary" style={{ fontSize: 12 }}>模型:</Text>
                    </Col>
                    <Col span={5}>
                        <Input value={model} onChange={(e) => setModel(e.target.value)} size="small" />
                    </Col>

                    <Col span={3}>
                        <Text type="secondary" style={{ fontSize: 12 }}>API Key:</Text>
                    </Col>
                    <Col span={13}>
                        <Input.Password prefix={<KeyOutlined />} value={apiKey} onChange={(e) => { setApiKey(e.target.value); setApiKeyConfigured(e.target.value.length > 0); }} placeholder="sk-..." size="small" />
                    </Col>
                    <Col span={8}>
                        <Button size="small" onClick={handleOpen} icon={<FolderOpenOutlined />}>打开项目</Button>
                        {projectName && <Tag color="blue" style={{ marginLeft: 4, fontSize: 11 }}>{projectName}</Tag>}
                    </Col>
                </Row>
            </Card>

            {/* ======== 执行控制 + 结果 ======== */}
            <Card size="small" style={{ marginBottom: 6 }}>
                <Space size={8}>
                    <Button type="primary" size="small"
                        icon={running && runningLabel === '数据汇总' ? <LoadingOutlined /> : <PlayCircleOutlined />}
                        onClick={handleSummary} disabled={btnSummaryDisabled || !dataFolder}
                        loading={running && runningLabel === '数据汇总'}>数据汇总</Button>
                    <Button size="small"
                        icon={running && runningLabel === '业态分析' ? <LoadingOutlined /> : <ThunderboltOutlined />}
                        onClick={handleSector} disabled={btnSectorDisabled || !apiKeyConfigured}
                        loading={running && runningLabel === '业态分析'}>业态分析</Button>
                    <Button size="small"
                        icon={running && runningLabel === '公司分析' ? <LoadingOutlined /> : <ReloadOutlined />}
                        onClick={handleCompany} disabled={btnCompanyDisabled || !apiKeyConfigured}
                        loading={running && runningLabel === '公司分析'}>公司分析</Button>
                    <Button size="small" type="primary"
                        icon={<FolderOpenOutlined />} onClick={handleOpenResult}
                        disabled={!allPhasesDone}
                        style={{ background: allPhasesDone ? '#52c41a' : undefined, borderColor: allPhasesDone ? '#52c41a' : undefined }}>
                        打开结果
                    </Button>
                    <Text type="secondary" style={{ fontSize: 11 }}>
                        {running ? `正在执行 - ${runningLabel}` : allPhasesDone ? '全部完成' : '就绪'}
                    </Text>
                </Space>
            </Card>

            {/* ======== 运行日志 ======== */}
            <Card size="small" bodyStyle={{ padding: 6 }}>
                <div style={{
                    height: 240, overflow: 'auto', background: '#1e1e1e', color: '#d4d4d4',
                    fontFamily: 'Consolas, "Courier New", monospace', fontSize: 12,
                    padding: 6, borderRadius: 4, whiteSpace: 'pre-wrap', wordBreak: 'break-all',
                }}>
                    {logLines.length === 0
                        ? <Text style={{ color: '#888', fontSize: 12 }}>等待执行...</Text>
                        : logLines.map((line, i) => <div key={i}>{line}</div>)
                    }
                    <div ref={logEndRef} />
                </div>
            </Card>
        </div>
    );
}
