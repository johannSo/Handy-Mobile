// What the keyboard setup needs from Android, provided by `SetupBridge` in
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
}

interface Bridge {
  status(): string;
  takeSetupRequest(): boolean;
  requestPermission(which: "microphone" | "localNetwork"): void;
  openKeyboardSettings(): void;
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

export const showKeyboardPicker = () => bridge()?.showKeyboardPicker();

const SEEN_KEY = "handy-mobile.keyboard-setup-seen";

/** Whether the setup was offered already, so it is only offered once. */
export const setupSeen = (): boolean => localStorage.getItem(SEEN_KEY) === "1";

export const markSetupSeen = () => localStorage.setItem(SEEN_KEY, "1");
