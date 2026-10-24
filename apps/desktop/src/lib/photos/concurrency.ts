export async function mapWithConcurrency<T, R>(items: readonly T[], limit: number, mapper: (item: T, index: number) => Promise<R>): Promise<R[]> {
  const results = Array.from<R>({ length: items.length });
  if (items.length === 0) return results;

  const workers = Math.max(1, Math.min(Math.floor(limit), items.length));
  let cursor = 0;
  const run = async (): Promise<void> => {
    while (true) {
      const index = cursor;
      cursor += 1;
      if (index >= items.length) return;
      results[index] = await mapper(items[index] as T, index);
    }
  };

  await Promise.all(Array.from({ length: workers }, () => run()));
  return results;
}
