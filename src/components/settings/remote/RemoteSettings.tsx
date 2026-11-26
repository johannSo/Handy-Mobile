import React from "react";
import { PhoneDictation } from "./PhoneDictation";
import { RemoteConnections } from "./RemoteConnections";
import { ShareThisComputer } from "./ShareThisComputer";
import { UseAnotherComputer } from "./UseAnotherComputer";

/**
 * The Handy Remote page: both directions of a remote connection in one
 * place. First transcribing with another computer, then letting phones and
 * computers transcribe with this one.
 */
export const RemoteSettings: React.FC = () => (
  <div className="max-w-3xl w-full mx-auto space-y-6">
    <UseAnotherComputer />
    <ShareThisComputer />
    <RemoteConnections />
    <PhoneDictation />
  </div>
);
