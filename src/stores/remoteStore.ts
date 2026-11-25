import { create } from "zustand";
import { commands, events, type RemoteStatus } from "@/bindings";

// Transcribing with another computer: which one, whether it can be reached,
// and the model it transcribes with. The backend sends the whole status on
// every change.
interface RemoteStore {
  status: RemoteStatus | null;
  initialized: boolean;
  initialize: () => Promise<void>;
  refresh: () => Promise<void>;
}

export const useRemoteStore = create<RemoteStore>()((set, get) => ({
  status: null,
  initialized: false,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });
    // Listen first, so a change between the two isn't missed; an event
    // that arrives before the first read is newer, and wins.
    let heard = false;
    await events.remoteStatus.listen(({ payload }) => {
      heard = true;
      set({ status: payload });
    });
    const status = await commands.getRemoteStatus();
    if (!heard) set({ status });
  },

  refresh: async () => {
    set({ status: await commands.getRemoteStatus() });
  },
}));

/** Another computer transcribes this one's dictation. */
export const useRemoteInUse = () =>
  useRemoteStore((s) => s.status?.in_use ?? false);

/** The selected computer's display name, if any. */
export const remoteServerName = (status: RemoteStatus | null) =>
  status?.servers.find((server) => server.id === status.selected)?.name ?? null;
