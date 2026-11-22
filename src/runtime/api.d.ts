export interface ExtensionContribution {
  buttonLabel?: string;
}

export interface ExtensionApi {
  log(event: string, detail?: Record<string, unknown>): void;
  contribute(value: ExtensionContribution): void;
}

export interface ExtensionModule {
  id?: string;
  setup?(api: ExtensionApi): void;
}
