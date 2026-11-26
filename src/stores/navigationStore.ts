import { create } from "zustand";

/** The pages the sidebar lists. */
export type SectionId =
  | "general"
  | "history"
  | "models"
  | "remote"
  | "advanced"
  | "postprocessing"
  | "debug"
  | "about";

// Which settings page is showing. A store rather than App's own state, so a
// sentence anywhere that names another page can link to it.
interface NavigationStore {
  section: SectionId;
  go: (section: SectionId) => void;
}

export const useNavigationStore = create<NavigationStore>()((set) => ({
  section: "general",
  go: (section) => set({ section }),
}));

/** Opens a settings page from outside a component. */
export const goTo = (section: SectionId) =>
  useNavigationStore.getState().go(section);
