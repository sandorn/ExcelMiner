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
    Divider,
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
import { listen } from '@tauri-apps/api/event';
import dayjs from 'dayjs';
import { useAppStore } from '../stores/appStore';
import type { Project, Company, AggregationResult, AnalysisResult, ProgressUpdate } from '../types';

const { Text } = Typography;

/** 格式化耗时 (与 AardMiner 一致: X分XX秒) */
function formatElapsed(ms: number): string {
    const sec = Math.floor(ms / 1000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}分${String(s).padStart(2, '0')}秒`;
}

/** 时间戳 HH:MM:SS */
function timestamp(): string {
    const d = new Date();
    return [d.getHours(), d.getMinutes(), d.getSeconds()]
        .map((n) => String(n).padStart(2, '0'))
        .join(':');
}

export default function MainPage() {
    const project = useAppStore((s) => s.project);
    const projectName = useAppStore((s) => s.projectName);
    const setProject = useAppStore((s) => s.setProject);
    const aggregationResults = useAppStore((s) => s.aggregationResults);
    const setAggregationResults = useAppStore((s) => s.setAggregationResults);
    const setAnalysisResults = useAppStore((s) => s.setAnalysisResults);

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
            message.error(`保存配置失败: ${e}`);
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
            message.error(`打开失败: ${e}`);
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

    // ── 阶段1: 数据汇总 ──
    const handleSummary = async () => {
        enableAll(); // 安全重置：清除之前可能残留的禁用状态
        const p = await saveConfig();
        if (!p) return;
        disableAll('数据汇总');
        const startTime = Date.now();
        addLog('=== 阶段1: 数据汇总 ===');

        try {
            const engines = ['insurance', 'hotel', 'commercial', 'financial'];
            const unlisten = await listen<{ step: string; progress: number; status: string }>(
                'aggregation-progress',
                (event) => { addLog(`  ${event.payload.step}`); },
            );
            try {
                const results = await invoke<AggregationResult[]>('execute_aggregation', {
                    project: p,
                    engines,
                });
                const runNames = new Set(['保险数据汇总', '酒店数据汇总', '商写数据汇总', '经营报表汇总']);
                setAggregationResults([
                    ...aggregationResults.filter((r) => !runNames.has(r.engine_name)),
                    ...results,
                ]);
                const elapsed = formatElapsed(Date.now() - startTime);
                addLog('');
                addLog(`=== 数据汇总完成，共耗时 ${elapsed} ===`);
            } finally {
                unlisten();
            }
        } catch (e: any) {
            addLog(`错误: ${e}`);
        } finally {
            enableAll();
        }
    };

    // ── 阶段2: 板块AI分析 ──
    const handleSector = async () => {
        enableAll(); // 安全重置：清除之前可能残留的禁用状态
        const p = await saveConfig();
        if (!p) return;
        if (!apiKey) {
            addLog('错误: 未配置 API Key，跳过板块分析');
            return;
        }
        disableAll('业态分析');
        const startTime = Date.now();
        addLog('=== 阶段2: 板块AI分析 ===');

        try {
            const unlisten = await listen<ProgressUpdate>('analysis-progress', (event) => {
                addLog(`  ${event.payload.step}`);
            });
            try {
                const results = await invoke<AnalysisResult[]>('execute_segment_analysis', {
                    project: p,
                    businessTypes: ['Commercial', 'Insurance', 'Hotel'],
                    customPrompt: null,
                });
                const current = useAppStore.getState().analysisResults;
                setAnalysisResults([
                    ...current.filter((r) => r.analysis_category === 'company'),
                    ...results,
                ]);
                const elapsed = formatElapsed(Date.now() - startTime);
                addLog('');
                addLog(`=== 业态分析完成，共耗时 ${elapsed} ===`);
            } finally {
                unlisten();
            }
        } catch (e: any) {
            addLog(`错误: ${e}`);
        } finally {
            enableAll();
        }
    };

    // ── 阶段3: 公司AI分析 ──
    const handleCompany = async () => {
        enableAll(); // 安全重置：清除之前可能残留的禁用状态
        const p = await saveConfig();
        if (!p) return;
        if (!apiKey) {
            addLog('错误: 未配置 API Key，跳过公司分析');
            return;
        }
        disableAll('公司分析');
        const startTime = Date.now();
        addLog('=== 阶段3: 公司核心指标分析 ===');

        try {
            const unlisten = await listen<ProgressUpdate>('analysis-progress', (event) => {
                addLog(`  ${event.payload.step}`);
            });
            try {
                const results = await invoke<AnalysisResult[]>('execute_company_analysis', {
                    project: p,
                });
                const current = useAppStore.getState().analysisResults;
                setAnalysisResults([
                    ...current.filter((r) => r.analysis_category === 'segment'),
                    ...results,
                ]);
                const elapsed = formatElapsed(Date.now() - startTime);
                addLog('');
                addLog(`=== 公司分析完成，共耗时 ${elapsed} ===`);
            } finally {
                unlisten();
            }
        } catch (e: any) {
            addLog(`错误: ${e}`);
        } finally {
            enableAll();
        }
    };

    return (
        <div style={{ maxWidth: 780, margin: '0 auto' }}>
            {/* ======== 运行配置 ======== */}
            <Card
                title={
                    <Text strong style={{ fontSize: 14 }}>
                        运行配置
                    </Text>
                }
                size="small"
                style={{ marginBottom: 8 }}
            >
                <Row gutter={[8, 4]} align="middle">
                    <Col span={4}>
                        <Text type="secondary">数据源目录:</Text>
                    </Col>
                    <Col span={18}>
                        <Input
                            value={dataFolder}
                            onChange={(e) => setDataFolder(e.target.value)}
                            placeholder="选择数据源目录..."
                            size="small"
                        />
                    </Col>
                    <Col span={2}>
                        <Button
                            size="small"
                            icon={<FolderOpenOutlined />}
                            onClick={() => selectFolder('data')}
                        />
                    </Col>

                    <Col span={4}>
                        <Text type="secondary">结果文件:</Text>
                    </Col>
                    <Col span={18}>
                        <Input
                            value={outputFile}
                            onChange={(e) => setOutputFile(e.target.value)}
                            placeholder="选择结果 .xlsx 文件..."
                            size="small"
                        />
                    </Col>
                    <Col span={2}>
                        <Button
                            size="small"
                            icon={<FolderOpenOutlined />}
                            onClick={() => selectFolder('output')}
                        />
                    </Col>

                    <Col span={4}>
                        <Text type="secondary">月份:</Text>
                    </Col>
                    <Col span={5}>
                        <DatePicker
                            picker="month"
                            value={month}
                            onChange={(d) => d && setMonth(d)}
                            format="M月"
                            size="small"
                            style={{ width: '100%' }}
                            allowClear={false}
                        />
                    </Col>
                    <Col span={3}>
                        <Text type="secondary">年份:</Text>
                    </Col>
                    <Col span={5}>
                        <DatePicker
                            picker="year"
                            value={month}
                            onChange={(d) => d && setMonth(d)}
                            format="YYYY年"
                            size="small"
                            style={{ width: '100%' }}
                            allowClear={false}
                        />
                    </Col>
                    <Col span={3}>
                        <Text type="secondary">模型:</Text>
                    </Col>
                    <Col span={4}>
                        <Input
                            value={model}
                            onChange={(e) => setModel(e.target.value)}
                            size="small"
                        />
                    </Col>

                    <Col span={4}>
                        <Text type="secondary">API Key:</Text>
                    </Col>
                    <Col span={14}>
                        <Input.Password
                            prefix={<KeyOutlined />}
                            value={apiKey}
                            onChange={(e) => {
                                setApiKey(e.target.value);
                                setApiKeyConfigured(e.target.value.length > 0);
                            }}
                            placeholder="sk-..."
                            size="small"
                        />
                    </Col>
                    <Col span={6}>
                        <Text
                            type={apiKeyConfigured ? 'success' : 'secondary'}
                            style={{ fontSize: 12 }}
                        >
                            API Key: {apiKeyConfigured ? '已配置' : '未配置'}
                        </Text>
                    </Col>
                </Row>

                <Divider style={{ margin: '6px 0' }} />

                <Space>
                    <Button size="small" onClick={handleOpen} icon={<FolderOpenOutlined />}>
                        打开已有项目
                    </Button>
                    {projectName && (
                        <Tag color="blue">当前项目: {projectName}</Tag>
                    )}
                </Space>
            </Card>

            {/* ======== 执行控制 ======== */}
            <Card
                title={
                    <Text strong style={{ fontSize: 14 }}>
                        执行控制
                    </Text>
                }
                size="small"
                style={{ marginBottom: 8 }}
            >
                <Space size={12}>
                    <Button
                        type="primary"
                        icon={running && runningLabel === '数据汇总' ? <LoadingOutlined /> : <PlayCircleOutlined />}
                        onClick={handleSummary}
                        disabled={btnSummaryDisabled || !dataFolder}
                        loading={running && runningLabel === '数据汇总'}
                    >
                        数据汇总
                    </Button>
                    <Button
                        icon={running && runningLabel === '业态分析' ? <LoadingOutlined /> : <ThunderboltOutlined />}
                        onClick={handleSector}
                        disabled={btnSectorDisabled || !apiKeyConfigured}
                        loading={running && runningLabel === '业态分析'}
                    >
                        业态分析
                    </Button>
                    <Button
                        icon={running && runningLabel === '公司分析' ? <LoadingOutlined /> : <ReloadOutlined />}
                        onClick={handleCompany}
                        disabled={btnCompanyDisabled || !apiKeyConfigured}
                        loading={running && runningLabel === '公司分析'}
                    >
                        公司分析
                    </Button>
                </Space>
            </Card>

            {/* ======== 运行日志 ======== */}
            <Card
                title={
                    <Text strong style={{ fontSize: 14 }}>
                        运行日志
                    </Text>
                }
                size="small"
                bodyStyle={{ padding: 8 }}
            >
                <div
                    style={{
                        height: 320,
                        overflow: 'auto',
                        background: '#1e1e1e',
                        color: '#d4d4d4',
                        fontFamily: 'Consolas, "Courier New", monospace',
                        fontSize: 13,
                        padding: 8,
                        borderRadius: 4,
                        whiteSpace: 'pre-wrap',
                        wordBreak: 'break-all',
                    }}
                >
                    {logLines.length === 0 ? (
                        <Text type="secondary" style={{ color: '#888' }}>
                            等待执行...
                        </Text>
                    ) : (
                        logLines.map((line, i) => (
                            <div key={i}>{line}</div>
                        ))
                    )}
                    <div ref={logEndRef} />
                </div>
            </Card>

            {/* ======== 状态栏 ======== */}
            <Text type="secondary" style={{ fontSize: 12 }}>
                状态: {running ? `正在执行 - ${runningLabel}` : '就绪'}
            </Text>
        </div>
    );
}
