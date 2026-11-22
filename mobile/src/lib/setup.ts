// What the bubble and keyboard setup needs from Android, provided by `SetupBridge` in
// MainActivity.kt. It doesn't exist anywhere else (a desktop browser, iOS),
// and there the keyboard isn't offered.

export interface SetupStatus {
  microphone: boolean;
  localNetwork: boolean;
  /** Whether this Android version asks about the local network at all. */
  localNetworkNeeded: boolean;
  /** Turned on in the system's keyboard list. */
  keyboardEnabled: boolean;
  /** The keyboard in use right now. */
  keyboardSelected: boolean;
  /** The floating bubble, turned on in the accessibility settings. */
  bubbleEnabled: boolean;
}

interface Bridge {
  status(): string;
  takeSetupRequest(): boolean;
  requestPermission(which: "microphone" | "localNetwork"): void;
  openKeyboardSettings(): void;
  openBubbleSettings(): void;
  openAppSettings(): void;
  showKeyboardPicker(): void;
}

const bridge = (): Bridge | undefined =>
  (window as unknown as { HandySetup?: Bridge }).HandySetup;

export const keyboardAvailable = (): boolean => bridge() !== undefined;

export const setupStatus = (): SetupStatus | null => {
  try {
    const json = bridge()?.status();
    return json ? (JSON.parse(json) as SetupStatus) : null;
  } catch {
    return null;
  }
};

/** True once after the keyboard sent the user to the app to finish setup. */
export const takeSetupRequest = (): boolean =>
  bridge()?.takeSetupRequest() ?? false;

export const requestPermission = (which: "microphone" | "localNetwork") =>
  bridge()?.requestPermission(which);

export const openKeyboardSettings = () => bridge()?.openKeyboardSettings();

export const openBubbleSettings = () => bridge()?.openBubbleSettings();

export const openAppSettings = () => bridge()?.openAppSettings();

export const showKeyboardPicker = () => bridge()?.showKeyboardPicker();

// Which setup the user was last offered. It grows when the setup gains a
// step, so that people who already went through it are shown the new one.
const SETUP_VERSION = 2;
const VERSION_KEY = "handy-mobile.setup-version";
/** How version 1, the keyboard alone, recorded that it was offered. */
const LEGACY_SEEN_KEY = "handy-mobile.keyboard-setup-seen";

/** The setup last offered: 0 for none, 1 before the bubble existed. */
export const setupVersionSeen = (): number => {
  const version = Number(localStorage.getItem(VERSION_KEY));
  if (version > 0) return version;
  return localStorage.getItem(LEGACY_SEEN_KEY) === "1" ? 1 : 0;
};

/** Whether the current setup was offered already, so it is only offered once. */
export const setupSeen = (): boolean => setupVersionSeen() >= SETUP_VERSION;

export const markSetupSeen = () =>
  localStorage.setItem(VERSION_KEY, String(SETUP_VERSION));
