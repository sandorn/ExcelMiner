import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
    Card,
    Form,
    Input,
    Select,
    Button,
    Space,
    DatePicker,
    Divider,
    Tag,
    message,
    Row,
    Col,
    Collapse,
    Flex,
} from 'antd';
import {
    PlusOutlined,
    FolderOpenOutlined,
    SaveOutlined,
    DeleteOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import dayjs from 'dayjs';
import { useAppStore } from '../stores/appStore';
import type { Project, Company, BusinessType } from '../types';

const BUSINESS_TYPES: { value: BusinessType; label: string }[] = [
    { value: 'Insurance', label: '保险' },
    { value: 'Hotel', label: '酒店' },
    { value: 'Commercial', label: '商写' },
];

export default function ProjectSetup() {
    const navigate = useNavigate();
    const setProject = useAppStore((s) => s.setProject);
    const [form] = Form.useForm();
    const [companies, setCompanies] = useState<Company[]>([
        { name: '北京中言', business_type: 'Commercial', regions: [] },
        { name: '大连凯丹', business_type: 'Commercial', regions: [] },
        { name: '福建钱隆', business_type: 'Commercial', regions: [] },
        { name: '春夏秋冬', business_type: 'Commercial', regions: [] },
        { name: '重庆宜新', business_type: 'Commercial', regions: [] },
        { name: '伯豪瑞廷', business_type: 'Hotel', regions: [] },
        { name: '重庆瑞尔', business_type: 'Hotel', regions: [] },
        { name: '盛唐融信', business_type: 'Insurance', regions: [] },
        { name: '君康经纪', business_type: 'Insurance', regions: [] },
    ]);
    const [saving, setSaving] = useState(false);
    const [loading, setLoading] = useState(false);

    const addCompany = () => {
        setCompanies([
            ...companies,
            { name: '', business_type: 'Insurance', regions: [] },
        ]);
    };

    const removeCompany = (idx: number) => {
        setCompanies(companies.filter((_, i) => i !== idx));
    };

    const updateCompany = (idx: number, field: keyof Company, value: any) => {
        const updated = [...companies];
        (updated[idx] as any)[field] = value;
        setCompanies(updated);
    };

    const handleCreate = async () => {
        try {
            const values = await form.validateFields();
            setSaving(true);

            const date = values.month as dayjs.Dayjs;
            const projectName = `${date.year()}年${date.month()}月`;

            const project = await invoke<Project>('create_project', {
                name: projectName,
                year: date.year(),
                month: date.month(),
                dataFolder: values.dataFolder,
                outputFile: `${values.outputFolder}/【${projectName}】经营数据.xlsx`,
            });

            // 更新项目中的公司列表
            const updated = {
                ...project,
                companies: companies.filter((c) => c.name.trim() !== ''),
            };
            await invoke('save_project', { project: updated });

            setProject(updated);
            message.success(`项目 "${projectName}" 创建成功`);
            navigate('/import');
        } catch (e: any) {
            message.error(`创建失败: ${e}`);
        } finally {
            setSaving(false);
        }
    };

    const handleOpen = async () => {
        try {
            setLoading(true);
            // 使用 Tauri dialog 打开文件
            const { open } = await import('@tauri-apps/plugin-dialog');
            const selected = await open({
                title: '选择项目配置文件',
                filters: [{ name: '项目配置', extensions: ['toml'] }],
                multiple: false,
            });
            if (selected) {
                const project = await invoke<Project>('open_project', {
                    path: selected as string,
                });
                setProject(project);
                form.setFieldsValue({
                    month: dayjs(
                        `${project.year}-${String(project.month).padStart(2, '0')}-01`,
                    ),
                    dataFolder: project.data_folder,
                    outputFolder: project.output_file.replace(/[^\\/]+$/, ''),
                });
                setCompanies(project.companies);
                message.success(`项目 "${project.name}" 已打开`);
            }
        } catch (e: any) {
            message.error(`打开失败: ${e}`);
        } finally {
            setLoading(false);
        }
    };

    const selectFolder = async (field: string) => {
        try {
            const { open } = await import('@tauri-apps/plugin-dialog');
            const selected = await open({
                title: '选择文件夹',
                directory: true,
                multiple: false,
            });
            if (selected) {
                form.setFieldValue(field, selected);
            }
        } catch (_) {
            /* dialog cancelled */
        }
    };

    return (
        <div className="page-container">
            <h2>📋 步骤一：项目设置</h2>

            <Card title="项目信息" size="small">
                <Form form={form} layout="vertical" style={{ maxWidth: 600 }}>
                    <Form.Item
                        name="month"
                        label="分析月份"
                        initialValue={dayjs()}
                        rules={[{ required: true, message: '请选择分析月份' }]}
                    >
                        <DatePicker
                            picker="month"
                            format="YYYY年M月"
                            style={{ width: '100%' }}
                        />
                    </Form.Item>
                    <Form.Item label="数据文件夹">
                        <Space.Compact style={{ width: '100%' }}>
                            <Form.Item
                                name="dataFolder"
                                noStyle
                                rules={[
                                    {
                                        required: true,
                                        message: '请选择子公司数据文件夹',
                                    },
                                ]}
                            >
                                <Input
                                    placeholder="如 D:/经营数据/2024年6月/"
                                    readOnly
                                />
                            </Form.Item>
                            <Button
                                icon={<FolderOpenOutlined />}
                                onClick={() => selectFolder('dataFolder')}
                            >
                                选择
                            </Button>
                        </Space.Compact>
                    </Form.Item>
                    <Form.Item label="输出文件夹">
                        <Space.Compact style={{ width: '100%' }}>
                            <Form.Item
                                name="outputFolder"
                                noStyle
                                rules={[
                                    {
                                        required: true,
                                        message: '请选择汇总输出文件夹',
                                    },
                                ]}
                            >
                                <Input
                                    placeholder="如 D:/经营数据/汇总/"
                                    readOnly
                                />
                            </Form.Item>
                            <Button
                                icon={<FolderOpenOutlined />}
                                onClick={() => selectFolder('outputFolder')}
                            >
                                选择
                            </Button>
                        </Space.Compact>
                    </Form.Item>
                </Form>
            </Card>

            <Card
                title="子公司配置"
                size="small"
                extra={
                    <Button
                        type="dashed"
                        icon={<PlusOutlined />}
                        onClick={addCompany}
                    >
                        添加公司
                    </Button>
                }
            >
                {companies.map((c, idx) => (
                    <Card
                        key={idx}
                        type="inner"
                        size="small"
                        style={{ marginBottom: 12 }}
                        title={`子公司 #${idx + 1}`}
                        extra={
                            <Button
                                type="text"
                                danger
                                icon={<DeleteOutlined />}
                                onClick={() => removeCompany(idx)}
                                disabled={companies.length <= 1}
                            />
                        }
                    >
                        <Row gutter={12}>
                            <Col span={14}>
                                <Form.Item
                                    label="公司名称"
                                    style={{ marginBottom: 0 }}
                                >
                                    <Input
                                        value={c.name}
                                        onChange={(e) =>
                                            updateCompany(
                                                idx,
                                                'name',
                                                e.target.value,
                                            )
                                        }
                                        placeholder="如 子公司A"
                                    />
                                </Form.Item>
                            </Col>
                            <Col span={10}>
                                <Form.Item
                                    label="业态"
                                    style={{ marginBottom: 0 }}
                                >
                                    <Select
                                        value={c.business_type}
                                        onChange={(v) =>
                                            updateCompany(
                                                idx,
                                                'business_type',
                                                v,
                                            )
                                        }
                                        options={BUSINESS_TYPES}
                                    />
                                </Form.Item>
                            </Col>
                        </Row>
                    </Card>
                ))}

                <Divider />

                <Flex justify="space-between">
                    <Button
                        icon={<FolderOpenOutlined />}
                        onClick={handleOpen}
                        loading={loading}
                    >
                        打开已有项目
                    </Button>
                    <Button
                        type="primary"
                        icon={<SaveOutlined />}
                        onClick={handleCreate}
                        loading={saving}
                    >
                        创建项目
                    </Button>
                </Flex>
            </Card>
        </div>
    );
}
