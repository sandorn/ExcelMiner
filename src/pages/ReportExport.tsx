import { useState } from 'react';
import {
    Card,
    Button,
    Space,
    Alert,
    Descriptions,
    Typography,
    Table,
    Tag,
    message,
    Empty,
} from 'antd';
import {
    ExportOutlined,
    CopyOutlined,
    FolderOpenOutlined,
    FileTextOutlined,
} from '@ant-design/icons';
import { useAppStore } from '../stores/appStore';

const { Paragraph, Text } = Typography;

export default function ReportExport() {
    const project = useAppStore((s) => s.project);
    const analysisResults = useAppStore((s) => s.analysisResults);
    const aggregationResults = useAppStore((s) => s.aggregationResults);

    const [exporting, setExporting] = useState(false);

    const handleOpenLog = async () => {
        try {
            const { invoke } = await import('@tauri-apps/api/core');
            const path = await invoke<string>('open_log_folder');
            message.success(`日志文件: ${path}`);
        } catch (e: any) {
            message.error(`打开日志失败: ${e}`);
        }
    };

    const handleExport = async () => {
        setExporting(true);
        try {
            const { invoke } = await import('@tauri-apps/api/core');
            const path = await invoke<string>('export_report');
            message.success(`报表已导出到: ${path}`);
        } catch (e: any) {
            message.error(`导出失败: ${e}`);
        } finally {
            setExporting(false);
        }
    };

    const handleCopyAll = async () => {
        const text = analysisResults
            .filter((r) => r.success)
            .map(
                (r) =>
                    r.analysis_category === 'segment'
                        ? `【${r.company_name} - 板块分析】\n${r.content}`
                        : `【${r.company_name} - 经营指标分析】\n${r.content}`,
            )
            .join('\n\n---\n\n');

        try {
            const { invoke } = await import('@tauri-apps/api/core');
            await invoke('copy_to_clipboard', { text });
            message.success('文案已复制到剪贴板');
        } catch (e: any) {
            // fallback: browser clipboard API
            try {
                await navigator.clipboard.writeText(text);
                message.success('文案已复制到剪贴板');
            } catch {
                message.error('复制失败，请手动复制');
            }
        }
    };

    const handleOpenFolder = async () => {
        if (!project) return;
        try {
            const { invoke } = await import('@tauri-apps/api/core');
            const folder = project.output_file.replace(/[^\\/]+$/, '');
            await invoke('open_in_explorer', { path: folder });
        } catch (e: any) {
            message.error(`打开文件夹失败: ${e}`);
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
            <h2>📤 步骤四：报表导出</h2>

            <Card title="项目摘要" size="small">
                <Descriptions column={2} size="small">
                    <Descriptions.Item label="项目名称">
                        {project.name}
                    </Descriptions.Item>
                    <Descriptions.Item label="分析月份">
                        {project.year}年{project.month}月
                    </Descriptions.Item>
                    <Descriptions.Item label="子公司数量">
                        {project.companies.length} 家
                    </Descriptions.Item>
                    <Descriptions.Item label="YTD月份数">
                        {project.ytd_months} 个月
                    </Descriptions.Item>
                    <Descriptions.Item label="数据文件夹" span={2}>
                        {project.data_folder}
                    </Descriptions.Item>
                    <Descriptions.Item label="输出文件" span={2}>
                        {project.output_file}
                    </Descriptions.Item>
                </Descriptions>
            </Card>

            <Card title="汇总数据" size="small" style={{ marginTop: 16 }}>
                {aggregationResults.length > 0 ? (
                    <Table
                        dataSource={aggregationResults}
                        rowKey="engine_name"
                        pagination={false}
                        columns={[
                            { title: '引擎', dataIndex: 'engine_name' },
                            {
                                title: '公司数',
                                dataIndex: 'companies_processed',
                            },
                            {
                                title: '指标数',
                                dataIndex: 'indicators_collected',
                            },
                            {
                                title: '警告',
                                render: (_, r) =>
                                    r.warnings.length > 0 ? (
                                        r.warnings.map(
                                            (w: string, i: number) => (
                                                <Tag key={i} color="warning">
                                                    {w}
                                                </Tag>
                                            ),
                                        )
                                    ) : (
                                        <Tag color="success">无</Tag>
                                    ),
                            },
                        ]}
                    />
                ) : (
                    <Empty description="暂无汇总数据，请先执行步骤二" />
                )}
            </Card>

            <Card title="AI分析文案" size="small" style={{ marginTop: 16 }}>
                {analysisResults.length > 0 ? (
                    <>
                        {analysisResults
                            .filter((r) => r.success)
                            .map((r, idx) => (
                                <Card
                                    key={idx}
                                    type="inner"
                                    size="small"
                                    className="result-card"
                                    title={
                                        <Space>
                                            <Tag
                                                color={
                                                    r.business_type === '保险'
                                                        ? 'green'
                                                        : r.business_type ===
                                                            '酒店'
                                                          ? 'blue'
                                                          : r.business_type ===
                                                              '经营指标'
                                                            ? 'purple'
                                                            : 'orange'
                                                }
                                            >
                                                {r.analysis_category === 'segment'
                                                    ? `${r.business_type}板块`
                                                    : '经营指标'}
                                            </Tag>
                                            <span>{r.company_name}</span>
                                            <Tag>评分 {r.quality_score}/10</Tag>
                                        </Space>
                                    }
                                >
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
                                </Card>
                            ))}
                    </>
                ) : (
                    <Empty description="暂无分析结果，请先执行步骤三" />
                )}
            </Card>

            <Space
                style={{
                    marginTop: 24,
                    width: '100%',
                    justifyContent: 'flex-end',
                }}
            >
                <Button
                    icon={<FolderOpenOutlined />}
                    onClick={handleOpenFolder}
                >
                    打开输出文件夹
                </Button>
                <Button icon={<FileTextOutlined />} onClick={handleOpenLog}>
                    查看日志
                </Button>
                <Button
                    icon={<CopyOutlined />}
                    onClick={handleCopyAll}
                    disabled={
                        analysisResults.filter((r) => r.success).length === 0
                    }
                >
                    复制全部文案
                </Button>
                <Button
                    type="primary"
                    icon={<ExportOutlined />}
                    onClick={handleExport}
                    loading={exporting}
                >
                    导出报表
                </Button>
            </Space>
        </div>
    );
}
