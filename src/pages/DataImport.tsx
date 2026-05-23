import { useState } from 'react';
import {
    Card,
    Checkbox,
    Button,
    Progress,
    Alert,
    Tag,
    Space,
    Table,
    Descriptions,
} from 'antd';
import {
    PlayCircleOutlined,
    SearchOutlined,
    CheckCircleOutlined,
    CloseCircleOutlined,
    LoadingOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../stores/appStore';
import type { PreviewData, AggregationResult } from '../types';

const BUSINESS_ENGINES = [
    {
        key: 'insurance',
        label: '保险数据汇总',
        desc: '人力、保费、续期、转化率',
        icon: '🟢',
    },
    {
        key: 'hotel',
        label: '酒店数据汇总',
        desc: '营销活动、OTA评分、入住率',
        icon: '🔵',
    },
    {
        key: 'commercial',
        label: '商写数据汇总',
        desc: '招商面积、渠道、续签率',
        icon: '🟠',
    },
];

const FINANCIAL_ENGINE = {
    key: 'financial',
    label: '经营报表汇总',
    desc: '通用财报指标（全部子公司）',
    icon: '🟣',
};

export default function DataImport() {
    const project = useAppStore((s) => s.project);
    const setAggregationResults = useAppStore((s) => s.setAggregationResults);

    const [selectedBusiness, setSelectedBusiness] = useState<string[]>([
        'insurance',
        'hotel',
        'commercial',
    ]);
    const [runFinancial, setRunFinancial] = useState(true);
    const [previewData, setPreviewData] = useState<Record<string, PreviewData>>(
        {},
    );
    const [previewing, setPreviewing] = useState(false);
    const [running, setRunning] = useState(false);
    const [progress, setProgress] = useState(0);
    const [statusText, setStatusText] = useState('');
    const [results, setResults] = useState<AggregationResult[]>([]);
    const [done, setDone] = useState(false);

    const handlePreview = async () => {
        setPreviewing(true);
        try {
            const engines = [...selectedBusiness];
            if (runFinancial) engines.push('financial');
            const allPreviews = await Promise.all(
                engines.map(async (engine) => {
                    const data = await invoke<PreviewData>('preview_import', {
                        project: project,
                        engine: engine,
                    });
                    return { engine, data };
                })
            );
            const newPreviewData: Record<string, PreviewData> = {};
            for (const { engine, data } of allPreviews) {
                newPreviewData[engine] = data;
            }
            setPreviewData(newPreviewData);
        } catch (e: any) {
            console.error('预览失败:', e);
        } finally {
            setPreviewing(false);
        }
    };

    const doAggregate = async (engines: string[]) => {
        setRunning(true);
        setProgress(0);
        setDone(false);

        const unlisten = await listen<{
            step: string;
            progress: number;
            status: string;
        }>('aggregation-progress', (event) => {
            setStatusText(event.payload.step);
            setProgress(Math.round(event.payload.progress * 100));
        });

        try {
            const newResults = await invoke<AggregationResult[]>(
                'execute_aggregation',
                {
                    project: project,
                    engines: engines,
                },
            );
            // merge: keep results from engines not in this run
            const runKeys = new Set(engines);
            const engineKeyToName: Record<string, string> = {
                insurance: '保险数据汇总',
                hotel: '酒店数据汇总',
                commercial: '商写数据汇总',
                financial: '经营报表汇总',
            };
            const runNames = new Set(
                engines.map((e) => engineKeyToName[e] || ''),
            );
            setResults((prev) => [
                ...prev.filter((r) => !runNames.has(r.engine_name)),
                ...newResults,
            ]);
            // update global state with merged results
            const merged = [
                ...results.filter((r) => !runNames.has(r.engine_name)),
                ...newResults,
            ];
            setAggregationResults(merged);
            setDone(true);
        } catch (e: any) {
            console.error('汇总失败:', e);
        } finally {
            unlisten();
            setRunning(false);
            setProgress(100);
        }
    };

    const handleBusinessAgg = () => doAggregate(selectedBusiness);
    const handleFinancialAgg = () => doAggregate(['financial']);

    if (!project) {
        return (
            <div className="page-container">
                <Alert
                    type="info"
                    message="请先完成项目设置"
                    description="请返回步骤一创建或打开一个项目"
                />
            </div>
        );
    }

    return (
        <div className="page-container">
            <h2>📥 步骤二：数据汇总</h2>

            <Card title="步骤一：经营数据汇总" size="small">
                <Checkbox.Group
                    value={selectedBusiness}
                    onChange={(v) => setSelectedBusiness(v as string[])}
                    style={{ width: '100%' }}
                >
                    <Space direction="vertical" style={{ width: '100%' }}>
                        {BUSINESS_ENGINES.map((eng) => (
                            <Card
                                key={eng.key}
                                size="small"
                                className="engine-card"
                                hoverable
                                style={{
                                    borderColor: selectedBusiness.includes(
                                        eng.key,
                                    )
                                        ? '#1677ff'
                                        : undefined,
                                }}
                            >
                                <Checkbox value={eng.key}>
                                    <Space>
                                        <span>{eng.icon}</span>
                                        <strong>{eng.label}</strong>
                                        <span
                                            style={{
                                                color: '#888',
                                                fontSize: 13,
                                            }}
                                        >
                                            {eng.desc}
                                        </span>
                                    </Space>
                                </Checkbox>
                            </Card>
                        ))}
                    </Space>
                </Checkbox.Group>
                <Button
                    type="primary"
                    icon={
                        running ? <LoadingOutlined /> : <PlayCircleOutlined />
                    }
                    onClick={handleBusinessAgg}
                    loading={running}
                    disabled={selectedBusiness.length === 0}
                    block
                    style={{ marginTop: 12, height: 40 }}
                >
                    执行经营数据汇总
                </Button>
            </Card>

            <Card title="步骤二：经营报表汇总" size="small" style={{ marginTop: 16 }}>
                <Checkbox
                    checked={runFinancial}
                    onChange={(e) => setRunFinancial(e.target.checked)}
                >
                    <Space>
                        <span>{FINANCIAL_ENGINE.icon}</span>
                        <strong>{FINANCIAL_ENGINE.label}</strong>
                        <span style={{ color: '#888', fontSize: 13 }}>
                            {FINANCIAL_ENGINE.desc}
                        </span>
                    </Space>
                </Checkbox>
                <Button
                    type="primary"
                    icon={
                        running ? <LoadingOutlined /> : <PlayCircleOutlined />
                    }
                    onClick={handleFinancialAgg}
                    loading={running}
                    disabled={!runFinancial}
                    block
                    style={{ marginTop: 12, height: 40 }}
                >
                    执行经营报表汇总
                </Button>
            </Card>

            <Space style={{ marginTop: 16 }}>
                <Button
                    icon={<SearchOutlined />}
                    onClick={handlePreview}
                    loading={previewing}
                >
                    预览全部数据
                </Button>
            </Space>

            {/* 预览结果 */}
            {Object.keys(previewData).length > 0 && (
                <Card title="预览结果" size="small" style={{ marginTop: 16 }}>
                    {Object.entries(previewData).map(([key, data]) => (
                        <Card
                            key={key}
                            type="inner"
                            size="small"
                            title={data.engine_name}
                            style={{ marginBottom: 8 }}
                        >
                            <Space wrap>
                                <Tag color="blue">
                                    文件 {data.files_found.length} 个
                                </Tag>
                                <Tag color="green">
                                    公司 {data.companies_detected.length} 家
                                </Tag>
                                <Tag color="orange">
                                    指标 {data.available_indicators.length} 项
                                </Tag>
                            </Space>
                            {data.warnings.length > 0 && (
                                <Alert
                                    type="warning"
                                    message={data.warnings.join('; ')}
                                    style={{ marginTop: 8 }}
                                />
                            )}
                        </Card>
                    ))}
                </Card>
            )}

            {/* 执行进度 */}
            {(running || done) && (
                <Card title="执行进度" size="small" style={{ marginTop: 16 }}>
                    <Progress
                        percent={progress}
                        status={done ? 'success' : 'active'}
                    />
                    <p style={{ marginTop: 8, color: '#666' }}>{statusText}</p>
                </Card>
            )}

            {/* 汇总结果 */}
            {results.length > 0 && (
                <Card title="汇总结果" size="small" style={{ marginTop: 16 }}>
                    <Table
                        dataSource={results}
                        rowKey="engine_name"
                        pagination={false}
                        columns={[
                            {
                                title: '汇总引擎',
                                dataIndex: 'engine_name',
                                key: 'engine_name',
                            },
                            {
                                title: '处理公司',
                                dataIndex: 'companies_processed',
                                key: 'cp',
                            },
                            {
                                title: '采集指标',
                                dataIndex: 'indicators_collected',
                                key: 'ic',
                            },
                            {
                                title: '状态',
                                key: 'status',
                                render: (_, r) =>
                                    r.warnings.length === 0 ? (
                                        <Tag
                                            icon={<CheckCircleOutlined />}
                                            color="success"
                                        >
                                            正常
                                        </Tag>
                                    ) : (
                                        <Tag
                                            icon={<CloseCircleOutlined />}
                                            color="warning"
                                        >
                                            {r.warnings.length} 条警告
                                        </Tag>
                                    ),
                            },
                        ]}
                    />
                </Card>
            )}
        </div>
    );
}
