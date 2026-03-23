import React from 'react';
import { ResponsiveContainer, AreaChart, Area, XAxis, YAxis, Tooltip, CartesianGrid } from 'recharts';

interface MetricChartProps {
  data: any[];
  dataKey: string;
  color: string;
  unit?: string;
}

export const MetricChart: React.FC<MetricChartProps> = ({ data, dataKey, color, unit = "" }) => {
  return (
    <div className="h-40 w-full mt-4 min-h-[160px]">
      <ResponsiveContainer width="100%" height={140} minWidth={0} minHeight={0}>
        <AreaChart data={data}>
          <defs>
            <linearGradient id={`gradient-${dataKey}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor={color} stopOpacity={0.3}/>
              <stop offset="95%" stopColor={color} stopOpacity={0}/>
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="#1e293b" />
          <XAxis 
            dataKey="time" 
            hide 
          />
          <YAxis 
            hide 
            domain={[0, 100]}
          />
          <Tooltip 
            contentStyle={{ backgroundColor: '#0f172a', borderColor: '#1e293b', color: '#f1f5f9' }}
            itemStyle={{ color: color }}
            formatter={(value: any) => [`${Number(value).toFixed(1)}${unit}`, dataKey]}
          />
          <Area 
            type="monotone" 
            dataKey={dataKey} 
            stroke={color} 
            fillOpacity={1} 
            fill={`url(#gradient-${dataKey})`} 
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
};
