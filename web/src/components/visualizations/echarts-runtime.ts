// Only this lazy-loaded module imports ECharts at runtime. Never import echarts/all.
import { init, use } from 'echarts/core';
import { LineChart, BarChart, HeatmapChart } from 'echarts/charts';
import { GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, VisualMapComponent, AriaComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
use([LineChart, BarChart, HeatmapChart, GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, VisualMapComponent, AriaComponent, CanvasRenderer]);
export { init };
