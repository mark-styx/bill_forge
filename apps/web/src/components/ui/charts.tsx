'use client';

import * as React from 'react';
import {
  AreaChart,
  Area,
  BarChart,
  Bar,
  LineChart,
  Line,
  PieChart,
  Pie,
  Cell,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  RadialBarChart,
  RadialBar,
} from 'recharts';
import { cn } from '@/lib/utils';
import { useThemeStore } from '@/stores/theme';

interface ChartContainerProps {
  children: React.ReactNode;
  className?: string;
  title?: string;
  description?: string;
  action?: React.ReactNode;
}

export function ChartContainer({
  children,
  className,
  title,
  description,
  action,
}: ChartContainerProps) {
  return (
    <div className={cn('card overflow-hidden', className)}>
      {(title || description || action) && (
        <div className="flex items-start justify-between p-5 pb-0">
          <div>
            {title && <h3 className="font-semibold text-foreground">{title}</h3>}
            {description && (
              <p className="text-sm text-muted-foreground mt-0.5">{description}</p>
            )}
          </div>
          {action}
        </div>
      )}
      <div className="p-5">{children}</div>
    </div>
  );
}

function useChartColors() {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  return {
    primary: `hsl(${colors.primary})`,
    accent: `hsl(${colors.accent})`,
    capture: `hsl(${colors.capture})`,
    processing: `hsl(${colors.processing})`,
    vendor: `hsl(${colors.vendor})`,
    reporting: `hsl(${colors.reporting})`,
    success: 'hsl(162 78% 42%)',
    warning: 'hsl(38 92% 50%)',
    error: 'hsl(0 84% 60%)',
    muted: 'hsl(var(--muted-foreground))',
    border: 'hsl(var(--border))',
    background: 'hsl(var(--background))',
  };
}

interface CustomTooltipProps {
  active?: boolean;
  payload?: any[];
  label?: string;
  formatter?: (value: number, name: string) => string;
}

function CustomTooltip({ active, payload, label, formatter }: CustomTooltipProps) {
  if (!active || !payload?.length) return null;

  return (
    <div className="bg-card border border-border rounded-lg shadow-lg p-3 text-sm">
      {label && <p className="font-medium text-foreground mb-2">{label}</p>}
      {payload.map((entry, index) => (
        <div key={index} className="flex items-center gap-2">
          <div
            className="w-2.5 h-2.5 rounded-full"
            style={{ backgroundColor: entry.color }}
          />
          <span className="text-muted-foreground">{entry.name}:</span>
          <span className="font-medium text-foreground">
            {formatter ? formatter(entry.value, entry.name) : entry.value}
          </span>
        </div>
      ))}
    </div>
  );
}

interface AreaChartProps {
  data: any[];
  dataKey: string;
  xAxisKey?: string;
  height?: number;
  showGrid?: boolean;
  gradient?: boolean;
  color?: 'primary' | 'accent' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'success';
  formatter?: (value: number) => string;
}

export function BillForgeAreaChart({
  data,
  dataKey,
  xAxisKey = 'name',
  height = 300,
  showGrid = true,
  gradient = true,
  color = 'primary',
  formatter,
}: AreaChartProps) {
  const chartColors = useChartColors();
  const fillColor = chartColors[color];
  const gradientId = `gradient-${color}-${Math.random().toString(36).substr(2, 9)}`;

  return (
    <ResponsiveContainer width="100%" height={height}>
      <AreaChart data={data} margin={{ top: 10, right: 10, left: 0, bottom: 0 }}>
        {gradient && (
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={fillColor} stopOpacity={0.3} />
              <stop offset="100%" stopColor={fillColor} stopOpacity={0} />
            </linearGradient>
          </defs>
        )}
        {showGrid && (
          <CartesianGrid
            strokeDasharray="3 3"
            stroke={chartColors.border}
            vertical={false}
          />
        )}
        <XAxis
          dataKey={xAxisKey}
          axisLine={false}
          tickLine={false}
          tick={{ fill: chartColors.muted, fontSize: 12 }}
          dy={10}
        />
        <YAxis
          axisLine={false}
          tickLine={false}
          tick={{ fill: chartColors.muted, fontSize: 12 }}
          tickFormatter={formatter}
          dx={-10}
        />
        <Tooltip
          content={
            <CustomTooltip formatter={formatter ? (v) => formatter(v) : undefined} />
          }
        />
        <Area
          type="monotone"
          dataKey={dataKey}
          stroke={fillColor}
          strokeWidth={2}
          fill={gradient ? `url(#${gradientId})` : fillColor}
          fillOpacity={gradient ? 1 : 0.1}
        />
      </AreaChart>
    </ResponsiveContainer>
  );
}

interface BarChartProps {
  data: any[];
  dataKey: string | string[];
  xAxisKey?: string;
  height?: number;
  showGrid?: boolean;
  stacked?: boolean;
  horizontal?: boolean;
  colors?: string[];
  formatter?: (value: number) => string;
  barRadius?: number;
}

export function BillForgeBarChart({
  data,
  dataKey,
  xAxisKey = 'name',
  height = 300,
  showGrid = true,
  stacked = false,
  horizontal = false,
  colors,
  formatter,
  barRadius = 4,
}: BarChartProps) {
  const chartColors = useChartColors();
  const defaultColors = [
    chartColors.primary,
    chartColors.accent,
    chartColors.capture,
    chartColors.processing,
    chartColors.vendor,
    chartColors.reporting,
  ];
  const barColors = colors || defaultColors;
  const dataKeys = Array.isArray(dataKey) ? dataKey : [dataKey];

  const ChartComponent = horizontal ? BarChart : BarChart;
  const layout = horizontal ? 'vertical' : 'horizontal';

  return (
    <ResponsiveContainer width="100%" height={height}>
      <ChartComponent
        data={data}
        layout={layout}
        margin={{ top: 10, right: 10, left: 0, bottom: 0 }}
      >
        {showGrid && (
          <CartesianGrid
            strokeDasharray="3 3"
            stroke={chartColors.border}
            horizontal={!horizontal}
            vertical={horizontal}
          />
        )}
        {horizontal ? (
          <>
            <XAxis type="number" axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} tickFormatter={formatter} />
            <YAxis type="category" dataKey={xAxisKey} axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} width={100} />
          </>
        ) : (
          <>
            <XAxis dataKey={xAxisKey} axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} dy={10} />
            <YAxis axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} tickFormatter={formatter} dx={-10} />
          </>
        )}
        <Tooltip content={<CustomTooltip formatter={formatter ? (v) => formatter(v) : undefined} />} />
        {dataKeys.length > 1 && <Legend />}
        {dataKeys.map((key, index) => (
          <Bar
            key={key}
            dataKey={key}
            fill={barColors[index % barColors.length]}
            stackId={stacked ? 'stack' : undefined}
            radius={[barRadius, barRadius, stacked && index < dataKeys.length - 1 ? 0 : barRadius, stacked && index < dataKeys.length - 1 ? 0 : barRadius]}
          />
        ))}
      </ChartComponent>
    </ResponsiveContainer>
  );
}

interface LineChartProps {
  data: any[];
  dataKey: string | string[];
  xAxisKey?: string;
  height?: number;
  showGrid?: boolean;
  showDots?: boolean;
  colors?: string[];
  formatter?: (value: number) => string;
  curved?: boolean;
}

export function BillForgeLineChart({
  data,
  dataKey,
  xAxisKey = 'name',
  height = 300,
  showGrid = true,
  showDots = false,
  colors,
  formatter,
  curved = true,
}: LineChartProps) {
  const chartColors = useChartColors();
  const defaultColors = [
    chartColors.primary,
    chartColors.accent,
    chartColors.capture,
    chartColors.processing,
  ];
  const lineColors = colors || defaultColors;
  const dataKeys = Array.isArray(dataKey) ? dataKey : [dataKey];

  return (
    <ResponsiveContainer width="100%" height={height}>
      <LineChart data={data} margin={{ top: 10, right: 10, left: 0, bottom: 0 }}>
        {showGrid && (
          <CartesianGrid strokeDasharray="3 3" stroke={chartColors.border} vertical={false} />
        )}
        <XAxis dataKey={xAxisKey} axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} dy={10} />
        <YAxis axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} tickFormatter={formatter} dx={-10} />
        <Tooltip content={<CustomTooltip formatter={formatter ? (v) => formatter(v) : undefined} />} />
        {dataKeys.length > 1 && <Legend />}
        {dataKeys.map((key, index) => (
          <Line
            key={key}
            type={curved ? 'monotone' : 'linear'}
            dataKey={key}
            stroke={lineColors[index % lineColors.length]}
            strokeWidth={2}
            dot={showDots}
            activeDot={{ r: 6, strokeWidth: 2, stroke: chartColors.background }}
          />
        ))}
      </LineChart>
    </ResponsiveContainer>
  );
}

interface DonutChartProps {
  data: { name: string; value: number; color?: string }[];
  height?: number;
  innerRadius?: number;
  outerRadius?: number;
  showLabels?: boolean;
  centerText?: string;
  centerValue?: string | number;
  formatter?: (value: number) => string;
}

export function BillForgeDonutChart({
  data,
  height = 300,
  innerRadius = 60,
  outerRadius = 100,
  showLabels = false,
  centerText,
  centerValue,
  formatter,
}: DonutChartProps) {
  const chartColors = useChartColors();
  const defaultColors = [
    chartColors.primary,
    chartColors.capture,
    chartColors.processing,
    chartColors.vendor,
    chartColors.reporting,
    chartColors.accent,
  ];

  const dataWithColors = data.map((item, index) => ({
    ...item,
    fill: item.color || defaultColors[index % defaultColors.length],
  }));

  return (
    <div className="relative">
      <ResponsiveContainer width="100%" height={height}>
        <PieChart>
          <Pie
            data={dataWithColors}
            cx="50%"
            cy="50%"
            innerRadius={innerRadius}
            outerRadius={outerRadius}
            paddingAngle={2}
            dataKey="value"
            stroke="none"
          >
            {dataWithColors.map((entry, index) => (
              <Cell key={`cell-${index}`} fill={entry.fill} />
            ))}
          </Pie>
          <Tooltip
            content={<CustomTooltip formatter={formatter ? (v) => formatter(v) : undefined} />}
          />
          {showLabels && <Legend />}
        </PieChart>
      </ResponsiveContainer>
      {(centerText || centerValue) && (
        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
          {centerValue && (
            <span className="text-2xl font-bold text-foreground">{centerValue}</span>
          )}
          {centerText && (
            <span className="text-sm text-muted-foreground">{centerText}</span>
          )}
        </div>
      )}
    </div>
  );
}

interface ProgressChartProps {
  value: number;
  max?: number;
  height?: number;
  color?: 'primary' | 'accent' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'success' | 'warning' | 'error';
  label?: string;
  showValue?: boolean;
}

export function BillForgeProgressChart({
  value,
  max = 100,
  height = 200,
  color = 'primary',
  label,
  showValue = true,
}: ProgressChartProps) {
  const chartColors = useChartColors();
  const fillColor = chartColors[color];
  const percentage = Math.min((value / max) * 100, 100);

  const data = [
    { name: label || 'Progress', value: percentage, fill: fillColor },
  ];

  return (
    <div className="relative">
      <ResponsiveContainer width="100%" height={height}>
        <RadialBarChart
          cx="50%"
          cy="50%"
          innerRadius="70%"
          outerRadius="100%"
          barSize={12}
          data={data}
          startAngle={90}
          endAngle={-270}
        >
          <RadialBar
            background={{ fill: chartColors.border }}
            dataKey="value"
            cornerRadius={6}
          />
        </RadialBarChart>
      </ResponsiveContainer>
      <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
        {showValue && (
          <span className="text-3xl font-bold text-foreground">{Math.round(percentage)}%</span>
        )}
        {label && (
          <span className="text-sm text-muted-foreground">{label}</span>
        )}
      </div>
    </div>
  );
}

interface SparklineProps {
  data: number[];
  width?: number;
  height?: number;
  color?: 'primary' | 'accent' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'success' | 'warning' | 'error';
  showArea?: boolean;
}

export function BillForgeSparkline({
  data,
  width = 100,
  height = 32,
  color = 'primary',
  showArea = false,
}: SparklineProps) {
  const chartColors = useChartColors();
  const strokeColor = chartColors[color];
  const gradientId = `sparkline-${Math.random().toString(36).substr(2, 9)}`;

  const chartData = data.map((value, index) => ({ index, value }));

  return (
    <div style={{ width, height }}>
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={chartData} margin={{ top: 2, right: 2, left: 2, bottom: 2 }}>
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={strokeColor} stopOpacity={0.3} />
              <stop offset="100%" stopColor={strokeColor} stopOpacity={0} />
            </linearGradient>
          </defs>
          <Area
            type="monotone"
            dataKey="value"
            stroke={strokeColor}
            strokeWidth={1.5}
            fill={showArea ? `url(#${gradientId})` : 'transparent'}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

interface MultiSeriesData {
  name: string;
  [key: string]: string | number;
}

interface StackedAreaChartProps {
  data: MultiSeriesData[];
  dataKeys: string[];
  xAxisKey?: string;
  height?: number;
  showGrid?: boolean;
  colors?: string[];
  formatter?: (value: number) => string;
}

export function BillForgeStackedAreaChart({
  data,
  dataKeys,
  xAxisKey = 'name',
  height = 300,
  showGrid = true,
  colors,
  formatter,
}: StackedAreaChartProps) {
  const chartColors = useChartColors();
  const defaultColors = [
    chartColors.primary,
    chartColors.capture,
    chartColors.processing,
    chartColors.vendor,
    chartColors.reporting,
    chartColors.accent,
  ];
  const areaColors = colors || defaultColors;

  return (
    <ResponsiveContainer width="100%" height={height}>
      <AreaChart data={data} margin={{ top: 10, right: 10, left: 0, bottom: 0 }}>
        <defs>
          {dataKeys.map((key, index) => (
            <linearGradient key={key} id={`gradient-${key}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={areaColors[index % areaColors.length]} stopOpacity={0.4} />
              <stop offset="100%" stopColor={areaColors[index % areaColors.length]} stopOpacity={0.1} />
            </linearGradient>
          ))}
        </defs>
        {showGrid && (
          <CartesianGrid strokeDasharray="3 3" stroke={chartColors.border} vertical={false} />
        )}
        <XAxis dataKey={xAxisKey} axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} dy={10} />
        <YAxis axisLine={false} tickLine={false} tick={{ fill: chartColors.muted, fontSize: 12 }} tickFormatter={formatter} dx={-10} />
        <Tooltip content={<CustomTooltip formatter={formatter ? (v) => formatter(v) : undefined} />} />
        <Legend />
        {dataKeys.map((key, index) => (
          <Area
            key={key}
            type="monotone"
            dataKey={key}
            stackId="1"
            stroke={areaColors[index % areaColors.length]}
            fill={`url(#gradient-${key})`}
          />
        ))}
      </AreaChart>
    </ResponsiveContainer>
  );
}
