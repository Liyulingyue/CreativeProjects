import React from 'react';
import { RadialBarChart, RadialBar, ResponsiveContainer, Tooltip } from 'recharts';

interface RadialMetricProps {
  value: number;
  label: string;
  subValue?: string;
  color: string;
  icon?: React.ReactNode;
}

const EmptyTooltip: React.FC = () => null;

export const RadialMetric: React.FC<RadialMetricProps> = ({ value, label, subValue, color, icon }) => {
  const data = [{ value, fill: color }];

  return (
    <div className="flex flex-col items-center">
      <div className="flex items-center gap-3 mb-6">
        <div className="p-2.5 bg-gray-50 rounded-2xl text-gray-900 shadow-sm border border-gray-100 flex items-center justify-center">
          {icon}
        </div>
        <div>
          <p className="text-[10px] uppercase font-black tracking-widest text-gray-400 leading-none">{label}</p>
          <h3 className="text-xl font-black text-gray-900 mt-1">{value.toFixed(1)}%</h3>
        </div>
      </div>
      
      <div className="relative w-full h-40">
        <ResponsiveContainer width="100%" height="100%">
          <RadialBarChart
            cx="50%"
            cy="50%"
            innerRadius="65%"
            outerRadius="100%"
            barSize={12}
            data={data}
            startAngle={180}
            endAngle={0}
          >
            <RadialBar
              background={{ fill: '#f3f4f6' }}
              dataKey="value"
              cornerRadius={30}
            />
            <Tooltip content={<EmptyTooltip />} />
          </RadialBarChart>
        </ResponsiveContainer>
        <div className="absolute top-[65%] left-1/2 -translate-x-1/2 -translate-y-1/2 text-center">
          <p className="text-[10px] font-black uppercase text-gray-400 tracking-tighter">{subValue}</p>
        </div>
      </div>
    </div>
  );
};
