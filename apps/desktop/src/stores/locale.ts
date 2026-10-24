import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { StorageKey } from "@/constants";
import { readStored, writeStored } from "@/lib/common/storage";

export type Locale = "zh-CN" | "en-US";

const MESSAGES: Record<string, { zh: string; en: string }> = {
  "brand.settings": { zh: "设置", en: "Settings" },
  "brand.themeSystem": { zh: "跟随系统", en: "System" },
  "brand.themeLight": { zh: "浅色", en: "Light" },
  "brand.themeDark": { zh: "深色", en: "Dark" },
  "nav.devices": { zh: "设备", en: "Devices" },
  "nav.photos": { zh: "照片", en: "Photos" },
  "header.photosLibrary": { zh: "iCloud 照片图库", en: "iCloud Photos" },
  "header.deviceCount": { zh: "共 {count} 台设备", en: "{count} devices" },
  "account.switch": { zh: "切换账号", en: "Switch account" },
  "account.checking": { zh: "正在检查…", en: "Checking…" },
  "account.known": { zh: "已知账号", en: "Known accounts" },
  "account.none": { zh: "还没有其他账号", en: "No other accounts" },
  "account.add": { zh: "添加账号", en: "Add account" },
  "account.logout": { zh: "退出当前账号", en: "Sign out" },
  "account.current": { zh: "当前", en: "Current" },
  "account.valid": { zh: "可用", en: "Available" },
  "account.expired": { zh: "需要重新登录", en: "Sign in again" },
  "account.unknown": { zh: "状态未知", en: "Unknown" },
  "theme.label": { zh: "主题", en: "Theme" },
  "theme.system": { zh: "跟随系统", en: "System" },
  "theme.light": { zh: "浅色", en: "Light" },
  "theme.dark": { zh: "深色", en: "Dark" },
  "login.title": { zh: "登录 iCloud", en: "Sign in to iCloud" },
  "login.subtitle": { zh: "登录后可浏览 iCloud 照片、查找设备，需完成双重认证。", en: "Sign in to view iCloud Photos and Find My devices. Two-factor authentication is required." },
  "login.reloginTitle": { zh: "该账号需要重新登录", en: "This account needs to sign in again" },
  "login.relogin": { zh: "重新登录该账号", en: "Sign in to this account" },
  "login.switchOther": { zh: "切换其他账号", en: "Use another account" },
  "login.appleId": { zh: "Apple ID", en: "Apple ID" },
  "login.password": { zh: "密码", en: "Password" },
  "login.passwordPlaceholder": { zh: "请输入密码", en: "Enter your password" },
  "login.china": { zh: "中国大陆账号", en: "Mainland China account" },
  "login.submit": { zh: "登录", en: "Sign in" },
  "login.authenticating": { zh: "正在认证…", en: "Signing in…" },
  "login.twoFactor": { zh: "双重认证", en: "Two-factor authentication" },
  "login.codeSent": { zh: "验证码已发送到受信任 Apple 设备", en: "A code was sent to your trusted Apple device" },
  "login.codeHelp": { zh: "在 iPhone 的 Apple ID 通知中查看 6 位验证码；在设备上点「允许」也会自动继续。", en: "Find the 6-digit code in the Apple ID notification on your iPhone. Approving on the device also continues automatically." },
  "login.waiting": { zh: "等待双重认证完成…", en: "Waiting for approval…" },
  "login.trustedDevices": { zh: "检测到 {count} 台受信任设备", en: "{count} trusted devices detected" },
  "login.verify": { zh: "验证并继续", en: "Verify and continue" },
  "login.resend": { zh: "重新发送验证码", en: "Resend code" },
  "login.resending": { zh: "正在发送…", en: "Sending…" },
  "login.reloginButton": { zh: "重新登录", en: "Sign in again" },
  "login.existingAccounts": { zh: "已有账号", en: "Existing accounts" },
  "login.checkingAccounts": { zh: "正在检查账号…", en: "Checking accounts…" },
  "login.addOther": { zh: "添加其他账号", en: "Add another account" },
  "settings.title": { zh: "设置", en: "Settings" },
  "settings.search": { zh: "搜索设置", en: "Search settings" },
  "settings.none": { zh: "没有匹配的设置项", en: "No matching settings" },
  "settings.appearance": { zh: "外观", en: "Appearance" },
  "settings.maps": { zh: "地图", en: "Maps" },
  "settings.downloads": { zh: "下载", en: "Downloads" },
  "settings.about": { zh: "关于", en: "About" },
  "settings.language": { zh: "语言 / Language", en: "Language" },
  "settings.chinese": { zh: "中文", en: "Chinese" },
  "settings.zoom": { zh: "界面缩放", en: "Interface scale" },
  "settings.theme": { zh: "主题", en: "Theme" },
  "settings.themeSaved": { zh: "设置会自动保存，下次打开继续使用。", en: "Changes are saved automatically." },
  "settings.downloadDesc": { zh: "控制新建下载任务时默认同时下载几张原图。", en: "Choose how many originals download concurrently by default." },
  "settings.concurrency": { zh: "默认并发数", en: "Default concurrency" },
  "settings.concurrentItems": { zh: "{count} 张", en: "{count} files" },
  "settings.concurrencyTip": { zh: "并发过高可能触发 Apple 限速，推荐 4 张。", en: "Too many concurrent downloads may trigger Apple rate limits. 4 is recommended." },
  "settings.downloadSubfolder": { zh: "独立任务文件夹", en: "Separate task folders" },
  "settings.downloadSubfolderTip": { zh: "开启后，每个下载任务会保存到单独文件夹，文件不会混在一起。", en: "Save each download task in its own folder so files do not mix." },
  "settings.mapsDesc": {
    zh: "选择地图提供商，并配置该服务需要的凭据。",
    en: "Choose a map provider and configure its credentials.",
  },
  "settings.mapProvider": { zh: "地图提供商", en: "Map provider" },
  "settings.mapCredentials": { zh: "地图凭据", en: "Map credentials" },
  "settings.mapCredentialsDesc": { zh: "{provider} 地图与逆地理编码配置", en: "Credentials for {provider} maps and reverse geocoding" },
  "settings.mapConfigured": { zh: "已配置", en: "Configured" },
  "settings.mapEnvironment": { zh: "使用环境变量", en: "Using environment" },
  "settings.mapMissing": { zh: "未配置", en: "Not configured" },
  "settings.mapSave": { zh: "保存", en: "Save" },
  "settings.mapClear": { zh: "清除本机配置", en: "Clear local settings" },
  "settings.mapSaved": { zh: "地图配置已保存，重新进入设备页后生效", en: "Map settings saved. Reopen Devices to apply." },
  "settings.mapCleared": { zh: "本机地图配置已清除", en: "Local map settings cleared" },
  "settings.mapHelpTitle": { zh: "如何配置", en: "How to configure" },
  "settings.mapOpenConsole": { zh: "打开服务商控制台", en: "Open provider console" },
  "settings.mapConsoleOpenFailed": { zh: "无法打开服务商控制台", en: "Could not open the provider console" },
  "settings.credentialShow": { zh: "显示凭据", en: "Show credential" },
  "settings.credentialHide": { zh: "隐藏凭据", en: "Hide credential" },
  "settings.amapHelpStep1": { zh: "在高德开放平台创建应用，添加 Web 端（JS API）Key。", en: "Create an app in the AMap console and add a Web JS API key." },
  "settings.amapHelpStep2": { zh: "按控制台要求配置域名白名单。", en: "Configure the domain allowlist as required by the console." },
  "settings.amapHelpStep3": { zh: "回到这里粘贴 Key；如果控制台提供安全密钥，也一并填写。", en: "Paste the key here, and enter the Security JS Code if provided." },
  "settings.amapProvider": { zh: "高德 JS API", en: "AMap JS API" },
  "settings.amapProviderDesc": { zh: "地图瓦片、设备标记与逆地理编码", en: "Map tiles, device markers, and reverse geocoding" },
  "settings.amapKey": { zh: "API Key", en: "API Key" },
  "settings.amapKeyTip": { zh: "本机 Key 不会同步到云端。", en: "A local key never syncs to the cloud." },
  "settings.amapEnvTip": { zh: "开发预览可在 .env.local 设置 VITE_AMAP_KEY；环境变量会内嵌到构建产物。", en: "For preview, set VITE_AMAP_KEY in .env.local. Environment values are bundled at build time." },
  "settings.amapPlaceholder": { zh: "粘贴高德 Web 端 JS API Key", en: "Paste an AMap Web JS API key" },
  "settings.amapSecurityCode": { zh: "安全密钥（Security JS Code）", en: "Security JS Code" },
  "settings.amapSecurityOptional": { zh: "可选", en: "Optional" },
  "settings.amapSecurityPlaceholder": { zh: "如果控制台提供了安全密钥，请填写", en: "Enter the Security JS Code if provided" },
  "settings.version": { zh: "版本", en: "Version" },
  "settings.sessionTtl": { zh: "会话有效期", en: "Session duration" },
  "settings.vault": { zh: "本机会话保存", en: "Local session storage" },
  "settings.vaultOn": { zh: "已启用加密保存", en: "Encrypted storage enabled" },
  "settings.vaultOff": { zh: "仅本次运行", en: "This session only" },
  "settings.hours": { zh: "{count} 小时", en: "{count} hours" },
  "settings.tagline": { zh: "iCloud 设备与照片，一处查看", en: "iCloud devices and photos, together" },
};

export const useLocaleStore = defineStore("locale", () => {
  const stored = readStored(StorageKey.locale);
  const locale = ref<Locale>(stored === "zh-CN" ? "zh-CN" : "en-US");
  const isEnglish = computed(() => locale.value === "en-US");
  const label = computed(() => (isEnglish.value ? "EN" : "中文"));

  function setLocale(next: Locale): void {
    locale.value = next;
    writeStored(StorageKey.locale, next);
  }

  function toggle(): void {
    setLocale(isEnglish.value ? "zh-CN" : "en-US");
  }

  function t(key: string, params?: Record<string, string | number>): string {
    const message = MESSAGES[key]?.[isEnglish.value ? "en" : "zh"] ?? key;
    if (!params) return message;
    return Object.entries(params).reduce((value, [name, replacement]) => value.replaceAll(`{${name}}`, String(replacement)), message);
  }

  return { locale, isEnglish, label, setLocale, toggle, t };
});
