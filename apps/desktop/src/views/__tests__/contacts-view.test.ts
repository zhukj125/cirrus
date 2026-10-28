import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import ContactsView from "@/views/ContactsView.vue";

const api = vi.hoisted(() => ({
  contactsList: vi.fn(),
  contactPhoto: vi.fn(),
  openExternal: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  contactsList: api.contactsList,
  contactPhoto: api.contactPhoto,
  openExternal: api.openExternal,
}));

function mountContacts() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(ContactsView, { props: { sessionId: "session-1" }, global: { plugins: [pinia] } });
}

describe("联系人页", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.contactPhoto.mockResolvedValue({ dataUrl: "data:image/png;base64,AAAA" });
    api.contactsList.mockResolvedValue({
      status: "OK",
      message: null,
      contacts: [
        {
          id: "contact-1",
          displayName: "朱凯杰",
          firstName: "凯杰",
          lastName: "朱",
          middleName: "",
          nickname: "",
          companyName: "",
          department: "",
          jobTitle: "",
          phones: [{ label: "MOBILE", value: "13800000000" }],
          emails: [{ label: "HOME", value: "me@example.com" }],
          addresses: [{ label: "HOME", street: "中山路 1 号", city: "杭州", region: "浙江", postalCode: "310000", country: "中国" }],
          urls: [],
          relatedNames: [],
          birthday: null,
          note: "测试备注",
          photoUrl: "https://gateway.icloud.com/contacts/489338457/ck/card/abc",
          isCompany: false,
        },
        {
          id: "contact-2",
          displayName: "Acme",
          firstName: "",
          lastName: "",
          middleName: "",
          nickname: "",
          companyName: "Acme",
          department: "Support",
          jobTitle: "Manager",
          phones: [{ label: "WORK", value: "400-800-1234" }],
          emails: [],
          addresses: [],
          urls: [{ label: "HOMEPAGE", value: "https://example.com" }],
          relatedNames: [],
          birthday: null,
          note: null,
          photoUrl: null,
          isCompany: true,
        },
      ],
    });
  });

  it("加载联系人、显示详情并支持搜索", async () => {
    const wrapper = mountContacts();
    await flushPromises();

    expect(api.contactsList).toHaveBeenCalledWith({ sessionId: "session-1", locale: "en_US" });
    expect(wrapper.text()).toContain("2 contacts");
    expect(wrapper.text()).toContain("朱凯杰");
    expect(wrapper.text()).toContain("13800000000");
    expect(wrapper.text()).toContain("me@example.com");
    expect(wrapper.text()).toContain("中山路 1 号");
    expect(wrapper.text()).toContain("测试备注");
    expect(api.contactPhoto).toHaveBeenCalledWith({ sessionId: "session-1", url: "https://gateway.icloud.com/contacts/489338457/ck/card/abc" });
    expect(wrapper.get("article img").attributes("src")).toBe("data:image/png;base64,AAAA");

    const search = wrapper.get('input[type="search"]');
    await search.setValue("400-800");
    await flushPromises();
    expect(wrapper.text()).toContain("Acme");
    expect(wrapper.text()).not.toContain("13800000000");
    expect(wrapper.get("h1").text()).toBe("Acme");
  });
});
