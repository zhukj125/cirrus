import { StorageKey } from "@/constants";
import { hasEnvironmentAmapKey } from "@/lib/amap";

export type MapCredentialIcon = "key" | "shield";

export interface MapCredentialField {
  id: string;
  storageKey: string;
  labelKey: string;
  placeholderKey: string;
  optionalKey?: string;
  helpKeys?: readonly string[];
  icon: MapCredentialIcon;
}

export interface MapProviderDefinition {
  id: string;
  name: string;
  descriptionKey: string;
  primaryFieldId: string;
  consoleUrl: string;
  fields: readonly MapCredentialField[];
  helpStepKeys: readonly string[];
  environmentConfigured?: () => boolean;
}

export const MAP_PROVIDERS: readonly MapProviderDefinition[] = [
  {
    id: "amap",
    name: "AMap",
    descriptionKey: "settings.amapProviderDesc",
    primaryFieldId: "apiKey",
    consoleUrl: "https://console.amap.com/dev/key/app",
    fields: [
      {
        id: "apiKey",
        storageKey: StorageKey.amapKey,
        labelKey: "settings.amapKey",
        placeholderKey: "settings.amapPlaceholder",
        helpKeys: ["settings.amapKeyTip", "settings.amapEnvTip"],
        icon: "key",
      },
      {
        id: "securityCode",
        storageKey: StorageKey.amapSecurityCode,
        labelKey: "settings.amapSecurityCode",
        placeholderKey: "settings.amapSecurityPlaceholder",
        optionalKey: "settings.amapSecurityOptional",
        icon: "shield",
      },
    ],
    helpStepKeys: ["settings.amapHelpStep1", "settings.amapHelpStep2", "settings.amapHelpStep3"],
    environmentConfigured: hasEnvironmentAmapKey,
  },
];

export const DEFAULT_MAP_PROVIDER_ID = MAP_PROVIDERS[0]!.id;

export function getMapProvider(id: string | null | undefined): MapProviderDefinition {
  return MAP_PROVIDERS.find((provider) => provider.id === id) ?? MAP_PROVIDERS[0]!;
}
