export interface CountLabelInput {
  shown: number;

  total: number | null;

  filtered: boolean;

  hasMore: boolean;

  truncated: boolean;

  exact?: number | null;

  exactLoading?: boolean;
}

export type Translator = (key: string, params?: Record<string, string | number>) => string;

function legacyCountLabel(input: CountLabelInput, english: boolean): string {
  if (input.filtered && input.exact !== null && input.exact !== undefined) {
    if (input.hasMore || input.truncated) return english ? `${input.shown} of ${input.exact} photos loaded in this range` : `当前范围已加载 ${input.shown} / ${input.exact} 张`;
    return english ? `${input.exact} photos in this range` : `当前范围共 ${input.exact} 张照片`;
  }
  if (input.filtered && input.exactLoading) {
    if (input.hasMore || input.truncated) return english ? `Calculating exact count… (${input.shown} loaded)` : `正在计算精确数量…（已加载 ${input.shown} 张）`;
    return english ? "Calculating exact count…" : "正在计算精确数量…";
  }
  if (!input.filtered && input.total !== null) return english ? `${input.shown} loaded / ${input.total} photos` : `已加载 ${input.shown} / 共 ${input.total} 张`;

  const scope = input.filtered ? (english ? "In this range: " : "当前范围：") : "";
  if (input.hasMore) {
    if (input.truncated) return english ? `${scope}${input.shown} loaded (more available)` : `${scope}已加载 ${input.shown} 张（还有更多）`;
    return english ? `${scope}${input.shown} loaded…` : `${scope}已加载 ${input.shown} 张…`;
  }
  return english ? `${scope}${input.shown} photos` : `${scope}${input.shown} 张照片`;
}

function translatedCountLabel(input: CountLabelInput, t: Translator): string {
  if (input.filtered && input.exact !== null && input.exact !== undefined) {
    if (input.hasMore || input.truncated) return t("photos.countRangeLoaded", { shown: input.shown, exact: input.exact });
    return t("photos.countRangeTotal", { exact: input.exact });
  }
  if (input.filtered && input.exactLoading) {
    if (input.hasMore || input.truncated) return t("photos.countCalculatingLoaded", { shown: input.shown });
    return t("photos.calculatingCount");
  }
  if (!input.filtered && input.total !== null) return t("photos.countLoadedTotal", { shown: input.shown, total: input.total });

  if (input.filtered) {
    if (input.hasMore) {
      if (input.truncated) return t("photos.countRangeMore", { shown: input.shown });
      return t("photos.countRangeLoading", { shown: input.shown });
    }
    return t("photos.countRangePhotos", { shown: input.shown });
  }
  if (input.hasMore) {
    if (input.truncated) return t("photos.countMore", { shown: input.shown });
    return t("photos.countLoading", { shown: input.shown });
  }
  return t("photos.countPhotos", { shown: input.shown });
}

export function countLabel(input: CountLabelInput, translator: boolean | Translator = true): string {
  return typeof translator === "boolean" ? legacyCountLabel(input, translator) : translatedCountLabel(input, translator);
}
