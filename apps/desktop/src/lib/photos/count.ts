export interface CountLabelInput {
  shown: number;

  total: number | null;

  filtered: boolean;

  hasMore: boolean;

  truncated: boolean;

  exact?: number | null;

  exactLoading?: boolean;
}

export function countLabel(input: CountLabelInput): string {
  if (input.filtered && input.exact !== null && input.exact !== undefined) {
    if (input.hasMore || input.truncated) return `${input.shown} of ${input.exact} photos loaded in this range`;
    return `${input.exact} photos in this range`;
  }
  if (input.filtered && input.exactLoading) {
    return input.hasMore || input.truncated ? `Calculating exact count… (${input.shown} loaded)` : "Calculating exact count…";
  }
  if (!input.filtered && input.total !== null) return `${input.shown} loaded / ${input.total} photos`;

  const scope = input.filtered ? "In this range: " : "";
  if (input.hasMore) {
    return input.truncated ? `${scope}${input.shown} loaded (more available)` : `${scope}${input.shown} loaded…`;
  }
  return `${scope}${input.shown} photos`;
}
