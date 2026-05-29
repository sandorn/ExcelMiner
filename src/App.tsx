import { useState, useEffect, useRef, useCallback } from 'react';
import { Layout, Typography, theme } from 'antd';
import { useAppStore } from './stores/appStore';
import MainPage from './pages/MainPage';

const { Header, Content } = Layout;
const { Text } = Typography;

function App() {
    const { token } = theme.useToken();
    const projectName = useAppStore((s) => s.projectName);

    return (
        <Layout style={{ minHeight: '100vh' }}>
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
                    <Text strong style={{ fontSize: 16, color: token.colorPrimary }}>
                        AardMiner
                    </Text>
                    <Text type="secondary" style={{ fontSize: 13 }}>
                        经营数据汇总分析工具
                    </Text>
                </div>
                {projectName && (
                    <Text type="secondary">
                        {projectName}
                    </Text>
                )}
            </Header>

            <Content
                style={{
                    padding: 16,
                    background: token.colorBgLayout,
                    minHeight: 'calc(100vh - 48px)',
                    overflow: 'auto',
                }}
            >
                <MainPage />
            </Content>
        </Layout>
    );
}

export default App;
