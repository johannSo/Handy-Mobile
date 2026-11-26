import { create } from "zustand";
import { commands, events, type MobileStatus } from "@/bindings";

// Remote connections to this computer: whether it listens for them, and the
// phones and computers paired with it. The backend only says that something
// changed, so every change is a fresh read.
interface MobileStore {
  status: MobileStatus | null;
  /** The device that paired last, until the pairing dialog has shown it. */
  pairedDevice: string | null;
  initialized: boolean;
  initialize: () => Promise<void>;
  refresh: () => Promise<void>;
  clearPaired: () => void;
}

export const useMobileStore = create<MobileStore>()((set, get) => ({
  status: null,
  pairedDevice: null,
  initialized: false,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });
    await events.mobileLinkChanged.listen(({ payload }) => {
      if (payload.paired_device) {
        set({ pairedDevice: payload.paired_device });
      }
      void get().refresh();
    });
    await get().refresh();
  },

  refresh: async () => {
    set({ status: await commands.getMobileStatus() });
  },

  clearPaired: () => set({ pairedDevice: null }),
}));
