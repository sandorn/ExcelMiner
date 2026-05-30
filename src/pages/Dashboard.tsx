import { useState, useEffect } from 'react';
import { Card, Row, Col, Statistic, Tag, Empty, Spin, Typography } from 'antd';
import { ArrowUpOutlined, ArrowDownOutlined, DashboardOutlined } from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { Column, Line, Pie } from '@ant-design/charts';

const { Text, Paragraph } = Typography;

// ── 类型定义 ──
interface KpiCard {
  title: string;
  value: number;
  unit: string;
  trend: string;
  color: string;
}
interface MonthlyPoint {
  month: string;
  company: string;
  value: number;
}
interface SegmentShare {
  segment: string;
  value: number;
}
interface CompanyBar {
  company: string;
  indicator: string;
  value: number;
}
interface AISummary {
  company: string;
  summary: string;
  score: number;
}
interface DashboardData {
  kpi_cards: KpiCard[];
  monthly_trend: MonthlyPoint[];
  segment_breakdown: SegmentShare[];
  company_comparison: CompanyBar[];
  ai_summaries: AISummary[];
}

// ── KPI 卡片 ──
function KpiCards({ cards }: { cards: KpiCard[] }) {
  return (
    <Row gutter={16} style={{ marginBottom: 16 }}>
      {cards.map((c, i) => (
        <Col span={6} key={i}>
          <Card size="small" hoverable>
            <Statistic
              title={c.title}
              value={c.value}
              suffix={c.unit}
              valueStyle={{ color: c.color, fontSize: 24 }}
              prefix={
                c.trend === 'up' ? (
                  <ArrowUpOutlined />
                ) : c.trend === 'down' ? (
                  <ArrowDownOutlined />
                ) : undefined
              }
            />
          </Card>
        </Col>
      ))}
    </Row>
  );
}

// ── YTD 月度趋势折线图 ──
function TrendChart({ data }: { data: MonthlyPoint[] }) {
  if (data.length === 0) return <Empty description="暂无趋势数据" />;
  const config = {
    data,
    xField: 'month',
    yField: 'value',
    seriesField: 'company',
    smooth: true,
    animation: { appear: { duration: 800 } },
    yAxis: { title: { text: '万元' } },
    legend: { position: 'bottom' as const },
    color: ['#1890ff', '#52c41a', '#fa8c16', '#722ed1', '#eb2f96'],
  };
  return <Line {...config} />;
}

// ── 业态营收占比饼图 ──
function SegmentPie({ data }: { data: SegmentShare[] }) {
  if (data.length === 0) return <Empty description="暂无业态数据" />;
  const config = {
    data,
    angleField: 'value',
    colorField: 'segment',
    radius: 0.8,
    innerRadius: 0.6,
    animation: { appear: { duration: 600 } },
    label: {
      type: 'outer' as const,
      content: '{name}\n{percentage}',
    },
    legend: { position: 'bottom' as const },
    color: ['#1890ff', '#52c41a', '#fa8c16'],
    statistic: {
      title: { content: '业态占比' },
    },
  };
  return <Pie {...config} />;
}

// ── 公司对比柱状图 ──
function CompanyBarChart({ data }: { data: CompanyBar[] }) {
  if (data.length === 0) return <Empty description="暂无对比数据" />;
  const config = {
    data,
    xField: 'company',
    yField: 'value',
    seriesField: 'indicator',
    isGroup: true,
    animation: { appear: { duration: 600 } },
    xAxis: { label: { autoRotate: true, autoHide: false } },
    yAxis: { title: { text: '万元' } },
    legend: { position: 'bottom' as const },
    color: ['#1890ff', '#52c41a'],
  };
  return <Column {...config} />;
}

// ── AI 分析摘要 ──
function AISummaryCards({ summaries }: { summaries: AISummary[] }) {
  if (summaries.length === 0) return <Empty description="暂无分析结果" />;
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {summaries.map((s, i) => (
        <Card key={i} size="small" type="inner">
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 4,
            }}
          >
            <Text strong>{s.company}</Text>
            <Tag color={s.score >= 7 ? 'green' : s.score >= 5 ? 'orange' : 'red'}>
              评分 {s.score}/10
            </Tag>
          </div>
          <Paragraph
            ellipsis={{ rows: 2, expandable: false }}
            style={{ marginBottom: 0, color: '#666', fontSize: 13 }}
          >
            {s.summary}
          </Paragraph>
        </Card>
      ))}
    </div>
  );
}

// ── 主组件 ──
export default function Dashboard() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    invoke<DashboardData>('get_dashboard_data')
      .then(setData)
      .catch((e) => console.error('加载仪表盘数据失败:', e))
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div style={{ textAlign: 'center', padding: 80 }}>
        <Spin size="large" tip="加载仪表盘数据..." />
      </div>
    );
  }

  if (!data) {
    return <Empty description="无法加载仪表盘数据" />;
  }

  return (
    <div style={{ padding: '8px 0' }}>
      {/* KPI 卡片 */}
      <KpiCards cards={data.kpi_cards} />

      {/* 图表行 */}
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col span={12}>
          <Card size="small" title="📈 YTD 月度营收趋势（演示）">
            <div style={{ height: 280 }}>
              <TrendChart data={data.monthly_trend} />
            </div>
          </Card>
        </Col>
        <Col span={12}>
          <Card size="small" title="🥧 业态营收占比（演示）">
            <div style={{ height: 280 }}>
              <SegmentPie data={data.segment_breakdown} />
            </div>
          </Card>
        </Col>
      </Row>

      {/* 公司对比 + AI 摘要 */}
      <Row gutter={16}>
        <Col span={14}>
          <Card size="small" title="📊 各公司营业收入对比（演示）">
            <div style={{ height: 300 }}>
              <CompanyBarChart data={data.company_comparison} />
            </div>
          </Card>
        </Col>
        <Col span={10}>
          <Card
            size="small"
            title={
              <span>
                <DashboardOutlined /> AI 分析摘要
              </span>
            }
          >
            <div style={{ maxHeight: 292, overflow: 'auto' }}>
              <AISummaryCards summaries={data.ai_summaries} />
            </div>
          </Card>
        </Col>
      </Row>
    </div>
  );
}
