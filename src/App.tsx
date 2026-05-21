import {
    Routes,
    Route,
    Navigate,
    useNavigate,
    useLocation,
} from 'react-router-dom';
import { Layout, Menu, Button, Typography, Steps, theme } from 'antd';
import {
    SettingOutlined,
    ImportOutlined,
    ThunderboltOutlined,
    ExportOutlined,
} from '@ant-design/icons';
import ProjectSetup from './pages/ProjectSetup';
import DataImport from './pages/DataImport';
import AIAnalysis from './pages/AIAnalysis';
import ReportExport from './pages/ReportExport';
import { useAppStore } from './stores/appStore';

const { Sider, Content, Header } = Layout;
const { Text } = Typography;

const steps = [
    { key: '/setup', icon: <SettingOutlined />, label: '项目设置' },
    { key: '/import', icon: <ImportOutlined />, label: '数据汇总' },
    { key: '/analysis', icon: <ThunderboltOutlined />, label: 'AI分析' },
    { key: '/export', icon: <ExportOutlined />, label: '报表导出' },
];

const currentStepIndex = (path: string) => {
    const idx = steps.findIndex((s) => path.startsWith(s.key));
    return idx >= 0 ? idx : 0;
};

function App() {
    const navigate = useNavigate();
    const location = useLocation();
    const { token } = theme.useToken();
    const projectName = useAppStore((s) => s.projectName);
    const current = currentStepIndex(location.pathname);

    return (
        <Layout style={{ minHeight: '100vh' }}>
            {/* 顶部标题栏 */}
            <Header
                style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    background: token.colorBgContainer,
                    borderBottom: `1px solid ${token.colorBorderSecondary}`,
                    padding: '0 24px',
                    height: 48,
                }}
            >
                <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                    <Text
                        strong
                        style={{ fontSize: 16, color: token.colorPrimary }}
                    >
                        📊 ExcelMiner
                    </Text>
                    <Text type="secondary" style={{ fontSize: 13 }}>
                        子公司经营数据汇总分析系统
                    </Text>
                </div>
                {projectName && <Text type="secondary">📁 {projectName}</Text>}
            </Header>

            <Layout>
                {/* 左侧步骤导航 */}
                <Sider
                    width={200}
                    style={{
                        background: token.colorBgContainer,
                        borderRight: `1px solid ${token.colorBorderSecondary}`,
                    }}
                >
                    <div style={{ padding: '16px 16px 12px' }}>
                        <Steps
                            direction="vertical"
                            size="small"
                            current={current}
                            items={steps.map((s, i) => ({
                                title: s.label,
                                icon: s.icon,
                                status:
                                    i === current
                                        ? 'process'
                                        : i < current
                                          ? 'finish'
                                          : 'wait',
                            }))}
                        />
                    </div>
                </Sider>

                {/* 右侧工作区 */}
                <Content
                    style={{
                        padding: 24,
                        background: token.colorBgLayout,
                        minHeight: 'calc(100vh - 48px)',
                        overflow: 'auto',
                    }}
                >
                    <Routes>
                        <Route path="/setup" element={<ProjectSetup />} />
                        <Route path="/import" element={<DataImport />} />
                        <Route path="/analysis" element={<AIAnalysis />} />
                        <Route path="/export" element={<ReportExport />} />
                        <Route
                            path="*"
                            element={<Navigate to="/setup" replace />}
                        />
                    </Routes>

                    {/* 底部导航按钮 */}
                    <div
                        style={{
                            display: 'flex',
                            justifyContent: 'space-between',
                            marginTop: 32,
                            paddingTop: 16,
                            borderTop: `1px solid ${token.colorBorderSecondary}`,
                        }}
                    >
                        <Button
                            disabled={current === 0}
                            onClick={() => navigate(steps[current - 1].key)}
                        >
                            上一步
                        </Button>
                        <Button
                            type="primary"
                            disabled={current === steps.length - 1}
                            onClick={() => navigate(steps[current + 1].key)}
                        >
                            下一步
                        </Button>
                    </div>
                </Content>
            </Layout>
        </Layout>
    );
}

export default App;
