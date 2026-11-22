export interface SidebarPageContent {
  title: string;
  body: string;
}

export interface SidebarItemInput {
  id: string;
  title: string;
  route: string;
  icon?: string;
  page: SidebarPageContent;
}

export interface ExtensionApi {
  log(event: string, detail?: Record<string, unknown>): void;
  ui: {
    registerSidebarItem(item: SidebarItemInput): () => void;
  };
}

export interface ExtensionModule {
  id?: string;
  setup?(api: ExtensionApi): void;
}
