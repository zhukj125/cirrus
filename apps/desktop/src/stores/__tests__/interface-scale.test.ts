import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { StorageKey } from "@/constants";
import { useInterfaceScaleStore } from "@/stores/interfaceScale";

describe("interface scale", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.style.removeProperty("zoom");
    setActivePinia(createPinia());
  });

  it("restores a saved scale and persists changes", async () => {
    localStorage.setItem(StorageKey.interfaceScale, "0.9");
    const store = useInterfaceScaleStore();

    expect(store.scale).toBe(0.9);
    expect(store.options.map((option) => option.label)).toEqual(["50%", "60%", "70%", "75%", "80%", "85%", "90%", "95%", "100%", "105%", "110%", "115%", "120%", "125%", "130%", "140%", "150%", "175%", "200%"]);

    await store.setScale("1.25");
    expect(store.scale).toBe(1.25);
    expect(localStorage.getItem(StorageKey.interfaceScale)).toBe("1.25");
    expect(document.documentElement.style.getPropertyValue("zoom")).toBe("1.25");
  });

  it("falls back to 100% for unsupported values", async () => {
    localStorage.setItem(StorageKey.interfaceScale, "133");
    const store = useInterfaceScaleStore();

    expect(store.scale).toBe(1);
    await store.setScale("133");
    expect(store.scale).toBe(1);
    expect(localStorage.getItem(StorageKey.interfaceScale)).toBe("1");
  });
});
