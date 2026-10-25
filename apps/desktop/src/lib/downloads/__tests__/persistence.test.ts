import { describe, expect, it } from "vitest";

import { parseDownloadTask, parseDownloadTasks, serializeDownloadTask } from "@/lib/downloads/persistence";
import type { DownloadTask } from "@/stores/downloads";
import type { PhotoItem } from "@/types";

function photo(id: string): PhotoItem {
  return {
    id,
    fileName: `${id}.jpeg`,
    takenAt: 1_700_000_000_000,
    isVideo: false,
    thumbUrl: `https://example.com/${id}.jpeg`,
    displayUrl: `https://example.com/${id}-display.jpeg`,
    originalUrl: `https://example.com/${id}-original.jpeg`,
    videoUrl: null,
    isLivePhoto: false,
  };
}

function rangeTask(): DownloadTask {
  return {
    accountKey: "dsid:1",
    id: "dl-1",
    title: "区间 · 2 张",
    directory: "C:\\out",
    status: "paused",
    source: {
      kind: "range",
      sessionId: "session-a",
      bounds: { after: 10, before: 20 },
      total: 2,
      startRank: 100,
      nextRank: 99,
      cursor: "cursor-1",
      pending: [photo("b")],
      exhausted: false,
      oldestTakenAt: 12,
      seen: new Set(["a", "b"]),
    },
    nextIndex: 0,
    done: 1,
    failed: ["bad.jpeg"],
    failedItems: [photo("bad")],
    completedFiles: [{ name: "a.jpeg", sizeBytes: 2048 }],
    usedNames: ["a.jpeg", "bad.jpeg"],
    concurrency: 4,
    createdAt: 123,
    error: null,
  };
}

describe("下载任务持久化 DTO", () => {
  it("区间任务往返时保留游标、pending、seen 与 usedNames", () => {
    const task = rangeTask();
    const raw = serializeDownloadTask(task);
    expect(raw.source).toMatchObject({ kind: "range", seen: ["a", "b"] });
    expect((raw.source as { pending: PhotoItem[] }).pending).toHaveLength(1);

    const parsed = parseDownloadTask(JSON.parse(JSON.stringify(raw)), "dsid:fallback");
    expect(parsed?.id).toBe("dl-1");
    expect(parsed?.source.kind).toBe("range");
    if (parsed?.source.kind === "range") {
      expect([...parsed.source.seen]).toEqual(["a", "b"]);
      expect(parsed.source.pending[0]?.id).toBe("b");
      expect(parsed.source.nextRank).toBe(99);
    }
    expect(parsed?.usedNames).toEqual(["a.jpeg", "bad.jpeg"]);
    expect(parsed?.completedFiles).toEqual([{ name: "a.jpeg", sizeBytes: 2048 }]);
  });

  it("快照任务往返时保留下一个下标与剩余照片", () => {
    const task: DownloadTask = {
      accountKey: "dsid:2",
      id: "dl-2",
      title: "选中的 2 张",
      directory: "C:/out",
      status: "paused",
      source: { kind: "snapshot", items: [photo("a"), photo("b")], pending: [] },
      nextIndex: 1,
      done: 1,
      failed: [],
      failedItems: [],
      usedNames: ["a.jpeg"],
      concurrency: 2,
      createdAt: 456,
      error: null,
    };
    const parsed = parseDownloadTask(serializeDownloadTask(task), "dsid:fallback");
    expect(parsed?.accountKey).toBe("dsid:2");
    expect(parsed?.source.kind).toBe("snapshot");
    expect(parsed?.nextIndex).toBe(1);
    expect(parsed?.source.kind === "snapshot" ? parsed.source.items.map((item) => item.id) : []).toEqual(["a", "b"]);
  });

  it("缺失账号 key 时使用当前账号回退值", () => {
    const raw = serializeDownloadTask(rangeTask());
    const parsed = parseDownloadTask({ ...raw, accountKey: undefined }, "dsid:fallback");
    expect(parsed?.accountKey).toBe("dsid:fallback");
  });

  it("非法条目与重复 ID 被丢弃", () => {
    const valid = serializeDownloadTask(rangeTask());
    const tasks = parseDownloadTasks([valid, { ...valid, title: "重复" }, { ...valid, id: "bad-source", source: { kind: "range", bounds: null } }, null], "dsid:1");
    expect(tasks).toHaveLength(1);
    expect(tasks[0]?.id).toBe("dl-1");
  });
});
