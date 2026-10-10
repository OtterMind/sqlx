import * as echarts from "echarts";
import "echarts-wordcloud";
import { element } from "./components";
import { chartOption, validateSpec, valueLabel } from "./chart-option";
import { resultSet } from "./result-set";
import type { ChartData, ChartSpec } from "../sdk/analytics";

export function renderChart(root: HTMLElement, spec: ChartSpec, data: ChartData): () => void {
  validateSpec(spec, data);
  root.replaceChildren();
  if (spec.chart_type === "Table") {
    // Every table in the product comes from the same component.
    root.append(resultSet({ columns: data.columns.map(name => ({ name })), rows: data.rows, label: spec.chart_type }).element);
    return () => {};
  }
  if (spec.chart_type === "Statistics") {
    const metric = spec.metrics[0];
    root.append(element("div", "stat-value", valueLabel(data.rows[0][data.columns.indexOf(metric.field)])), element("div", "stat-unit", metric.unit));
    return () => {};
  }
  const chart = echarts.init(root, undefined, { renderer: "canvas" });
  const paint = () => chart.setOption(chartOption(spec, data, document.documentElement.dataset.theme === "dark"), true);
  paint();
  const size = new ResizeObserver(() => chart.resize()); size.observe(root);
  const theme = new MutationObserver(paint); theme.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  return () => { size.disconnect(); theme.disconnect(); chart.dispose(); };
}
