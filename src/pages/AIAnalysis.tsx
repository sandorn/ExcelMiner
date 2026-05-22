import { useState } from 'react';
import {
    Card,
    Checkbox,
    Button,
    Input,
    Progress,
    Tag,
    Space,
    Collapse,
    Typography,
    Alert,
    Descriptions,
} from 'antd';
import {
    ThunderboltOutlined,
    KeyOutlined,
    ApartmentOutlined,
    ReloadOutlined,
    CheckCircleOutlined,
    CloseCircleOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../stores/appStore';
import type { AnalysisResult, ProgressUpdate } from '../types';

const { TextArea } = Input;
const { Text, Paragraph } = Typography;

const BUSINESS_CHECKBOXES = [
    { key: 'Insurance', label: '保险业态', desc: '2家公司对比分析' },
    { key: 'Hotel', label: '酒店业态', desc: '区域+公司对比分析' },
    { key: 'Commercial', label: '商写业态', desc: '5家公司对比分析' },
];

export default function AIAnalysis() {
    const project = useAppStore((s) => s.project);
    const setAnalysisResults = useAppStore((s) => s.setAnalysisResults);

    const [selectedTypes, setSelectedTypes] = useState<string[]>([
        'Insurance',
        'Hotel',
        'Commercial',
    ]);
    const [apiKey, setApiKey] = useState('');
    const [systemPrompt, setSystemPrompt] = useState('');
    const [running, setRunning] = useState(false);
    const [progress, setProgress] = useState(0);
    const [statusText, setStatusText] = useState('');
    const [currentCompany, setCurrentCompany] = useState('');
    const [results, setResults] = useState<AnalysisResult[]>([]);

    // 测试连接
    const handleTestConnection = async () => {
        try {
            const response = await invoke<string>('test_api_connection', {
                apiUrl:
                    project?.ai_config?.api_url ??
                    'https://api.deepseek.com/v1/chat/completions',
                apiKey: apiKey,
                model: project?.ai_config?.model ?? 'deepseek-chat',
            });
            alert(`连接成功：${response}`);
        } catch (e: any) {
            alert(`连接失败：${e}`);
        }
    };

    const handleExecute = async () => {
        if (!project) return;

        // 更新 API key
        const updatedProject = {
            ...project,
            ai_config: { ...project.ai_config, api_key: apiKey },
        };

        setRunning(true);
        setProgress(0);
        setResults([]);

        const unlisten = await listen<ProgressUpdate>(
            'analysis-progress',
            (event) => {
                setStatusText(event.payload.step);
                setProgress(Math.round(event.payload.progress * 100));
                if (event.payload.company)
                    setCurrentCompany(event.payload.company);
            },
        );

        try {
            const result = await invoke<AnalysisResult[]>('execute_analysis', {
                project: updatedProject,
                businessTypes: selectedTypes,
                customPrompt: systemPrompt || null,
            });
            setResults(result);
            setAnalysisResults(result);
        } catch (e: any) {
            console.error('分析失败:', e);
        } finally {
            unlisten();
            setRunning(false);
        }
    };

    if (!project) {
        return (
            <div className="page-container">
                <Alert
                    type="info"
                    message="请先完成项目设置"
                    description="请返回步骤一创建或打开项目"
                />
            </div>
        );
    }

    return (
        <div className="page-container">
            <h2>🤖 步骤三：AI业态分析</h2>

            <Card title="API 配置" size="small">
                <Space direction="vertical" style={{ width: '100%' }}>
                    <Input.Password
                        prefix={<KeyOutlined />}
                        placeholder="DeepSeek API Key (sk-...)"
                        value={apiKey}
                        onChange={(e) => setApiKey(e.target.value)}
                    />
                    <Space>
                        <Text type="secondary">
                            API地址: {project.ai_config?.api_url}
                        </Text>
                        <Text type="secondary">
                            模型: {project.ai_config?.model}
                        </Text>
                    </Space>
                    <Button
                        icon={<ThunderboltOutlined />}
                        onClick={handleTestConnection}
                    >
                        测试连接
                    </Button>
                </Space>
            </Card>

            <Card title="系统提示词" size="small" style={{ marginTop: 16 }}>
                <TextArea
                    rows={6}
                    placeholder="请输入AI分析的系统提示词，或留空使用默认提示词..."
                    value={systemPrompt}
                    onChange={(e) => setSystemPrompt(e.target.value)}
                />
            </Card>

            <Card title="选择分析业态" size="small" style={{ marginTop: 16 }}>
                <Checkbox.Group
                    value={selectedTypes}
                    onChange={(v) => setSelectedTypes(v as string[])}
                >
                    <Space direction="vertical" style={{ width: '100%' }}>
                        {BUSINESS_CHECKBOXES.map((b) => (
                            <Card
                                key={b.key}
                                size="small"
                                hoverable
                                className="engine-card"
                            >
                                <Checkbox value={b.key}>
                                    <strong>{b.label}</strong>
                                    <span
                                        style={{
                                            color: '#888',
                                            marginLeft: 8,
                                            fontSize: 13,
                                        }}
                                    >
                                        {b.desc}
                                    </span>
                                </Checkbox>
                            </Card>
                        ))}
                    </Space>
                </Checkbox.Group>
            </Card>

            <Button
                type="primary"
                icon={<ApartmentOutlined />}
                onClick={handleExecute}
                loading={running}
                disabled={selectedTypes.length === 0 || !apiKey}
                block
                style={{ marginTop: 16, height: 44 }}
            >
                执行分析
            </Button>

            {running && (
                <Card size="small" style={{ marginTop: 16 }}>
                    <Progress percent={progress} status="active" />
                    <Text style={{ display: 'block', marginTop: 8 }}>
                        {statusText}
                    </Text>
                    {currentCompany && (
                        <Tag color="processing">{currentCompany}</Tag>
                    )}
                </Card>
            )}

            {results.length > 0 && (
                <Card title="分析结果" size="small" style={{ marginTop: 16 }}>
                    {results.map((r, idx) => (
                        <Card
                            key={idx}
                            type="inner"
                            size="small"
                            className="result-card"
                            title={
                                <Space>
                                    {r.success ? (
                                        <CheckCircleOutlined
                                            style={{ color: '#52c41a' }}
                                        />
                                    ) : (
                                        <CloseCircleOutlined
                                            style={{ color: '#ff4d4f' }}
                                        />
                                    )}
                                    <span>{r.company_name}</span>
                                    <Tag
                                        color={
                                            r.business_type === '保险'
                                                ? 'green'
                                                : r.business_type === '酒店'
                                                  ? 'blue'
                                                  : 'orange'
                                        }
                                    >
                                        {r.business_type}
                                    </Tag>
                                    <Tag
                                        color={
                                            r.quality_score >= 8
                                                ? 'success'
                                                : r.quality_score >= 6
                                                  ? 'warning'
                                                  : 'error'
                                        }
                                    >
                                        评分: {r.quality_score}/10
                                    </Tag>
                                    {r.retry_count > 0 && (
                                        <Tag>重试 {r.retry_count} 次</Tag>
                                    )}
                                </Space>
                            }
                            extra={
                                r.token_usage && (
                                    <Text
                                        type="secondary"
                                        style={{ fontSize: 12 }}
                                    >
                                        tokens: {r.token_usage.total_tokens}
                                    </Text>
                                )
                            }
                        >
                            {r.error_message ? (
                                <Alert type="error" message={r.error_message} />
                            ) : (
                                <Paragraph
                                    ellipsis={{
                                        rows: 3,
                                        expandable: true,
                                        symbol: '展开',
                                    }}
                                    style={{ whiteSpace: 'pre-wrap' }}
                                >
                                    {r.content}
                                </Paragraph>
                            )}
                        </Card>
                    ))}
                </Card>
            )}
        </div>
    );
}
