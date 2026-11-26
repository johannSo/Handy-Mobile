import type { RemoteServer, RemoteStatus } from "@/bindings";

type Translate = (key: string, options?: Record<string, unknown>) => string;

/** What is said about the link to the computer this one transcribes with. */
export const linkText = (status: RemoteStatus, t: Translate): string => {
  switch (status.link) {
    case "connected":
      if (status.busy) return t("settings.remote.state.transcribing");
      return status.model
        ? t("settings.remote.state.connectedModel", {
            model: status.model.name,
          })
        : t("settings.remote.state.noModel");
    case "connecting":
      return t("settings.remote.state.connecting");
    case "offline":
      return t("settings.remote.state.offline");
    case "rejected":
      return t("settings.remote.state.rejected");
    case "incompatible":
      return t("settings.remote.state.incompatible");
    case "unused":
      return t("settings.remote.state.unused");
  }
};

/** The dot's color for a link state, as the phone app draws it. */
export const linkDot = (status: RemoteStatus): string => {
  switch (status.link) {
    case "connected":
      return status.model ? "bg-moss" : "bg-warning";
    case "connecting":
      return "bg-warning animate-pulse motion-reduce:animate-none";
    case "unused":
      return "bg-ink-3";
    default:
      return "bg-error";
  }
};

/**
 * What is said about a paired computer that isn't the one in use: whether it
 * answered when last looked for, so switching to it isn't a gamble. `null`
 * until it has been looked for.
 */
export const reachText = (server: RemoteServer, t: Translate): string | null =>
  server.reachable === null
    ? null
    : server.reachable
      ? t("settings.remote.state.available")
      : t("settings.remote.state.offline");

export const reachDot = (server: RemoteServer): string | null =>
  server.reachable === null ? null : server.reachable ? "bg-moss" : "bg-error";
