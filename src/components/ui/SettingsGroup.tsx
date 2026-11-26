import React from "react";

interface SettingsGroupProps {
  title?: string;
  description?: React.ReactNode;
  /** Beside the title: a link to where the group's subject is managed. */
  action?: React.ReactNode;
  children: React.ReactNode;
}

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  description,
  action,
  children,
}) => {
  return (
    <div className="space-y-2">
      {title && (
        <div className="px-4">
          <div className="flex items-baseline justify-between gap-3">
            <h2 className="text-xs font-medium text-ink-3 uppercase tracking-wide">
              {title}
            </h2>
            {action}
          </div>
          {description && (
            <p className="text-xs text-ink-3 mt-1">{description}</p>
          )}
        </div>
      )}
      <div className="bg-background border border-line rounded-lg overflow-visible">
        <div className="divide-y divide-line">{children}</div>
      </div>
    </div>
  );
};
