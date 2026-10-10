import type { EChartsOption, SeriesOption } from "echarts";
import type { ChartData, ChartSpec, Metric, SeriesType } from "../sdk/analytics";
import { t } from "./i18n";

export const chartLabels = {
  Column: t("chart.type.column"), Bar: t("chart.type.bar"), Line: t("chart.type.line"), AreaLine: t("chart.type.areaLine"), Scatter: t("chart.type.scatter"),
  Pie: t("chart.type.pie"), RingPie: t("chart.type.ringPie"), RosePie: t("chart.type.rosePie"), Funnel: t("chart.type.funnel"), WordCloud: t("chart.type.wordCloud"),
  Statistics: t("chart.type.statistics"), Combo: t("chart.type.combo"), Table: t("chart.type.table"),
};
export const tupleKey = (values: unknown[]): string => JSON.stringify(values.map(v => [v === null ? "null" : typeof v, v]));
export const valueLabel = (value: unknown): string => value === null ? t("common.null") : value === "" ? t("chart.value.empty") : typeof value === "object" ? JSON.stringify(value) : String(value);
export function numberValue(value: unknown, safe = true): number | null {
  if (value === null) return null;
  if (typeof value !== "number" && (typeof value !== "string" || !/^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(value))) throw new Error(t("chart.error.metricNotNumeric", { label: valueLabel(value) }));
  const number = Number(value);
  if (!Number.isFinite(number) || (safe && Math.abs(number) > Number.MAX_SAFE_INTEGER)) throw new Error(t("chart.error.unsafeNumber"));
  return number;
}
export function validateSpec(spec: ChartSpec, data: ChartData): void {
  if (new Set(data.columns).size !== data.columns.length) throw new Error(t("chart.error.duplicateColumns"));
  if (spec.group_by.length > 3 || spec.metrics.length > 8) throw new Error(t("chart.error.tooManyFields"));
  const fields = [...(spec.dimension ? [spec.dimension] : []), ...spec.group_by, ...spec.metrics.map(m => m.field)];
  if (fields.some(f => !data.columns.includes(f)) || new Set(fields).size !== fields.length) throw new Error(t("chart.error.chooseField"));
  if (spec.chart_type === "Table") return;
  if (!spec.metrics.length) throw new Error(t("chart.error.needMetric"));
  if (spec.chart_type === "Statistics") {
    if (data.rows.length !== 1 || spec.metrics.length !== 1 || spec.group_by.length) throw new Error(t("chart.error.statisticsShape"));
  } else if (!spec.dimension) throw new Error(t("chart.error.needDimension"));
  const radial = ["Pie", "RingPie", "RosePie", "Funnel", "WordCloud"].includes(spec.chart_type);
  if (radial && (spec.metrics.length !== 1 || spec.group_by.length)) throw new Error(t("chart.error.singleMetric"));
  for (const axis of ["left", "right"]) if (new Set(spec.metrics.filter(m => m.axis === axis).map(m => m.unit)).size > 1) throw new Error(t("chart.error.mixedUnits"));
  if (spec.chart_type !== "Combo" && spec.metrics.some(m => m.axis === "right" || m.kind !== null)) throw new Error(t("chart.error.useCombo"));
  if (spec.chart_type === "Combo" && spec.metrics.some(m => m.kind === null)) throw new Error(t("chart.error.comboSeries"));
  const groups = new Set<string>(), grain = new Set<string>();
  for (const row of data.rows) {
    if (spec.chart_type === "Scatter") numberValue(row[data.columns.indexOf(spec.dimension!)]);
    const group = spec.group_by.map(f => row[data.columns.indexOf(f)]);
    groups.add(tupleKey(group));
    if (groups.size * spec.metrics.length > 32) throw new Error(t("chart.error.tooManySeries"));
    if (!["Scatter", "Statistics"].includes(spec.chart_type)) {
      const key = tupleKey([row[data.columns.indexOf(spec.dimension!)], ...group]);
      if (grain.has(key)) throw new Error(t("chart.error.duplicateGrain"));
      grain.add(key);
    }
    for (const metric of spec.metrics) {
      const value = numberValue(row[data.columns.indexOf(metric.field)], spec.chart_type !== "Statistics");
      if (radial && value !== null && value < 0) throw new Error(t("chart.error.negative"));
    }
  }
}

/** Shared pure renderer for preview and saved charts. Missing observations remain null. */
export function chartOption(spec: ChartSpec, data: ChartData, dark = false): EChartsOption {
  validateSpec(spec, data);
  const text = dark ? "#dce3ed" : "#394258";
  const option: EChartsOption = {
    color: ["#7161ef", "#16a7a0", "#f7b955", "#ea739d", "#5888e6", "#ab79d6", "#70b984", "#db8855"],
    animationDuration: 250, textStyle: { color: text }, backgroundColor: "transparent",
    tooltip: { trigger: "item", renderMode: "richText", confine: true },
    legend: { type: "scroll", top: 0, textStyle: { color: text } },
  };
  const at = (row: unknown[], field: string) => row[data.columns.indexOf(field)];
  if (["Pie", "RingPie", "RosePie", "Funnel", "WordCloud"].includes(spec.chart_type)) {
    const entries = data.rows.map(row => ({ name: valueLabel(at(row, spec.dimension!)), value: numberValue(at(row, spec.metrics[0].field)) ?? 0 }));
    if (spec.chart_type === "WordCloud") {
      option.legend = undefined;
      // The extension registers its own series with ECharts at runtime.
      option.series = [{ type: "wordCloud", shape: "circle", left: "center", top: "center", width: "96%", height: "94%", sizeRange: [14, 64], rotationRange: [0, 0], gridSize: 6, drawOutOfBound: false, layoutAnimation: false, textStyle: { color: "#7161ef" }, data: entries }];
    } else if (spec.chart_type === "Funnel") {
      option.series = [{ type: "funnel", left: "8%", width: "84%", top: 40, bottom: 12, sort: "none", label: { color: text, position: "inside" }, data: entries }];
    } else option.series = [{ type: "pie", radius: spec.chart_type === "RingPie" ? ["42%", "68%"] : "68%", center: ["50%", "56%"], ...(spec.chart_type === "RosePie" ? { roseType: "radius" as const } : {}), label: { color: text }, data: entries }];
    return option;
  }
  const groups = new Map<string, { label: string; rows: unknown[][] }>();
  const labels = new Map<string, number>();
  for (const row of data.rows) {
    const values = spec.group_by.map(f => at(row, f)), key = tupleKey(values);
    if (!groups.has(key)) {
      const base = values.map(valueLabel).join(" / ");
      const count = (labels.get(base) ?? 0) + 1; labels.set(base, count);
      groups.set(key, { label: count > 1 ? `${base} (${count})` : base, rows: [] });
    }
    groups.get(key)!.rows.push(row);
  }
  const categories = new Map<string, string>();
  const categoryLabels = new Map<string, number>();
  for (const row of data.rows) {
    const value = at(row, spec.dimension!), key = tupleKey([value]);
    if (categories.has(key)) continue;
    const base = valueLabel(value), count = (categoryLabels.get(base) ?? 0) + 1;
    categoryLabels.set(base, count); categories.set(key, count > 1 ? `${base} (${count})` : base);
  }
  const categoryKeys = [...categories.keys()];
  const horizontal = spec.chart_type === "Bar", scatter = spec.chart_type === "Scatter";
  const axis = (side: Metric["axis"]) => ({ type: "value" as const, name: spec.metrics.find(m => m.axis === side)?.unit ?? "", axisLabel: { color: text }, splitLine: { lineStyle: { color: dark ? "#30384a" : "#edf0f5" } } });
  const categoryAxis = { type: "category" as const, data: [...categories.values()], axisLabel: { color: text, hideOverlap: true }, axisTick: { show: false } };
  option.grid = { left: 24, right: 28, top: 54, bottom: 20, containLabel: true };
  option.xAxis = scatter ? axis("left") : horizontal ? { ...axis("left"), nameLocation: "middle", nameGap: 26 } : categoryAxis;
  option.yAxis = horizontal ? categoryAxis : [axis("left"), ...(spec.metrics.some(m => m.axis === "right") ? [axis("right")] : [])];
  option.tooltip = { trigger: scatter ? "item" : "axis", renderMode: "richText", confine: true };
  const series: SeriesOption[] = [];
  const seriesNames = new Map<string, number>();
  for (const [groupKey, group] of groups) for (const metric of spec.metrics) {
    const kind: SeriesType = spec.chart_type === "Combo" ? metric.kind! : spec.chart_type === "Column" || horizontal ? "Column" : scatter ? "Scatter" : spec.chart_type === "AreaLine" ? "AreaLine" : "Line";
    const rows = new Map(group.rows.map(row => [tupleKey([at(row, spec.dimension!)]), row]));
    const values = scatter ? group.rows.map(row => [numberValue(at(row, spec.dimension!)), numberValue(at(row, metric.field))]) : categoryKeys.map(key => rows.has(key) ? numberValue(at(rows.get(key)!, metric.field)) : null);
    const base = [group.label, metric.label || metric.field].filter(Boolean).join(" · ");
    const count = (seriesNames.get(base) ?? 0) + 1; seriesNames.set(base, count);
    const common = { id: tupleKey([groupKey, metric.field]), name: count > 1 ? `${base} (${count})` : base, data: values, yAxisIndex: horizontal ? 0 : metric.axis === "right" ? 1 : 0 };
    const stack = spec.stack && kind !== "Scatter" && kind !== "Line" ? tupleKey([metric.field, kind, metric.axis, metric.unit]) : undefined;
    if (kind === "Column") series.push({ ...common, type: "bar", stack, barMaxWidth: 32 });
    else if (kind === "Scatter") series.push({ ...common, type: "scatter", symbolSize: 8 });
    else series.push({ ...common, type: "line", stack, smooth: spec.line_style === "smooth", step: spec.line_style === "step" ? "end" : undefined, connectNulls: false, showSymbol: data.rows.length < 120, ...(kind === "AreaLine" ? { areaStyle: { opacity: .18 } } : {}) });
  }
  option.series = series;
  return option;
}
