/**
 * The design-system primitives.
 *
 * Everything visual in OVERLAY is built from these, so spacing, radii,
 * motion and focus behaviour stay consistent without each page re-deciding.
 * They read design tokens from `themes/theme.css` and never hard-code a
 * colour, which is what lets the theme switch without touching components.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";

export function cx(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(" ");
}

// --------------------------------------------------------------------------
// Button
// --------------------------------------------------------------------------

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: ReactNode;
  loading?: boolean;
  full?: boolean;
}

const BUTTON_SIZES: Record<ButtonSize, string> = {
  sm: "h-7 px-2.5 text-[12px] gap-1.5 rounded-[6px]",
  md: "h-9 px-3.5 text-[13px] gap-2 rounded-[8px]",
  lg: "h-11 px-5 text-[14px] gap-2 rounded-[10px]",
};

export function Button({
  variant = "secondary",
  size = "md",
  icon,
  loading,
  full,
  className,
  children,
  disabled,
  ...rest
}: ButtonProps) {
  const base =
    "no-drag inline-flex items-center justify-center font-medium whitespace-nowrap " +
    "transition-all duration-[var(--dur-fast)] disabled:opacity-40 " +
    "disabled:pointer-events-none active:scale-[0.985] select-none";

  const variants: Record<ButtonVariant, string> = {
    primary:
      "text-[var(--text-inverse)] shadow-[var(--shadow-sm)] " +
      "bg-[var(--accent-500)] hover:bg-[var(--accent-400)]",
    secondary:
      "text-[var(--text-primary)] bg-[var(--bg-raised)] " +
      "border border-[var(--border-default)] hover:bg-[var(--bg-hover)] " +
      "hover:border-[var(--border-strong)]",
    ghost:
      "text-[var(--text-secondary)] hover:text-[var(--text-primary)] " +
      "hover:bg-[var(--bg-hover)]",
    danger:
      "text-[var(--danger)] bg-[var(--danger-dim)] " +
      "border border-transparent hover:border-[var(--danger)]",
  };

  return (
    <button
      className={cx(base, BUTTON_SIZES[size], variants[variant], full && "w-full", className)}
      disabled={disabled || loading}
      {...rest}
    >
      {loading ? <Spinner size={size === "sm" ? 12 : 14} /> : icon}
      {children}
    </button>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
      style={{ animation: "spin 0.7s linear infinite" }}
    >
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
      <path
        d="M21 12a9 9 0 0 0-9-9"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
      />
      <style>{"@keyframes spin{to{transform:rotate(360deg)}}"}</style>
    </svg>
  );
}

// --------------------------------------------------------------------------
// Toggle
// --------------------------------------------------------------------------

interface ToggleProps {
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label?: string;
  /** Rendered as the accessible name when there is no visible label. */
  srLabel?: string;
}

export function Toggle({ checked, onChange, disabled, label, srLabel }: ToggleProps) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={srLabel ?? label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "no-drag relative inline-flex h-[22px] w-[38px] shrink-0 items-center rounded-full",
        "transition-colors duration-[var(--dur)] disabled:opacity-40",
        "disabled:pointer-events-none border",
        checked
          ? "bg-[var(--accent-500)] border-[var(--accent-500)]"
          : "bg-[var(--bg-active)] border-[var(--border-default)]",
      )}
    >
      <span
        className="absolute h-[16px] w-[16px] rounded-full bg-white shadow-[var(--shadow-sm)]"
        style={{
          transform: `translateX(${checked ? 19 : 3}px)`,
          transition: "transform var(--dur) var(--ease)",
        }}
      />
    </button>
  );
}

// --------------------------------------------------------------------------
// Cards
// --------------------------------------------------------------------------

export function Card({
  className,
  children,
  interactive,
  ...rest
}: { interactive?: boolean } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cx(
        "card",
        interactive &&
          "cursor-pointer transition-all duration-[var(--dur-fast)] " +
            "hover:border-[var(--border-strong)] hover:-translate-y-px",
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
}

export function SectionTitle({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="mb-3 flex items-end justify-between gap-4">
      <div>
        <h2 className="text-[13px] font-semibold tracking-[0.02em] text-[var(--text-primary)]">
          {title}
        </h2>
        {description && (
          <p className="mt-0.5 text-[12px] leading-snug text-[var(--text-tertiary)]">
            {description}
          </p>
        )}
      </div>
      {action}
    </div>
  );
}

// --------------------------------------------------------------------------
// Status indicators
// --------------------------------------------------------------------------

export type Tone = "neutral" | "ok" | "warn" | "danger" | "accent";

const TONE_STYLES: Record<Tone, string> = {
  neutral:
    "text-[var(--text-tertiary)] bg-[var(--bg-active)] border-[var(--border-subtle)]",
  ok: "text-[var(--ok)] bg-[var(--ok-dim)] border-transparent",
  warn: "text-[var(--warn)] bg-[var(--warn-dim)] border-transparent",
  danger: "text-[var(--danger)] bg-[var(--danger-dim)] border-transparent",
  accent:
    "text-[var(--accent-400)] bg-[rgb(var(--accent-rgb)/0.12)] border-transparent",
};

export function Badge({
  children,
  tone = "neutral",
  className,
}: {
  children: ReactNode;
  tone?: Tone;
  className?: string;
}) {
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1 rounded-full border px-2 py-[2px]",
        "text-[11px] font-medium leading-none",
        TONE_STYLES[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

export function Dot({ tone = "neutral" }: { tone?: Tone }) {
  const colors: Record<Tone, string> = {
    neutral: "var(--text-tertiary)",
    ok: "var(--ok)",
    warn: "var(--warn)",
    danger: "var(--danger)",
    accent: "var(--accent-500)",
  };
  return (
    <span
      className="inline-block h-[6px] w-[6px] shrink-0 rounded-full"
      style={{ background: colors[tone] }}
      aria-hidden
    />
  );
}

// --------------------------------------------------------------------------
// Form controls
// --------------------------------------------------------------------------

export function Field({
  label,
  hint,
  children,
  htmlFor,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
  htmlFor?: string;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label
        htmlFor={htmlFor}
        className="text-[12px] font-medium text-[var(--text-secondary)]"
      >
        {label}
      </label>
      {children}
      {hint && <p className="text-[11px] text-[var(--text-tertiary)]">{hint}</p>}
    </div>
  );
}

export function TextInput({
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cx(
        "no-drag h-9 w-full rounded-[8px] border border-[var(--border-default)]",
        "bg-[var(--bg-sunken)] px-3 text-[13px] text-[var(--text-primary)]",
        "placeholder:text-[var(--text-tertiary)] transition-colors",
        "hover:border-[var(--border-strong)] focus:border-[var(--accent-500)]",
        className,
      )}
      {...rest}
    />
  );
}

export function Slider({
  value,
  onChange,
  min = 0,
  max = 100,
  step = 1,
  format,
  disabled,
}: {
  value: number;
  onChange: (next: number) => void;
  min?: number;
  max?: number;
  step?: number;
  format?: (v: number) => string;
  disabled?: boolean;
}) {
  const pct = max === min ? 0 : ((value - min) / (max - min)) * 100;
  return (
    <div className="flex items-center gap-3">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(Number(e.target.value))}
        className="no-drag h-1.5 flex-1 cursor-pointer appearance-none rounded-full disabled:opacity-40"
        style={{
          background: `linear-gradient(to right, var(--accent-500) ${pct}%, var(--bg-active) ${pct}%)`,
        }}
      />
      <span className="tabular w-12 text-right text-[12px] text-[var(--text-secondary)]">
        {format ? format(value) : value}
      </span>
      <style>{`
        input[type="range"]::-webkit-slider-thumb {
          appearance: none;
          width: 14px; height: 14px;
          border-radius: 50%;
          background: #fff;
          box-shadow: var(--shadow-sm);
          cursor: pointer;
          transition: transform var(--dur-fast) var(--ease);
        }
        input[type="range"]:hover::-webkit-slider-thumb { transform: scale(1.15); }
      `}</style>
    </div>
  );
}

export function Select<T extends string>({
  value,
  onChange,
  options,
  disabled,
  id,
}: {
  value: T;
  onChange: (next: T) => void;
  options: Array<{ value: T; label: string }>;
  disabled?: boolean;
  id?: string;
}) {
  return (
    <select
      id={id}
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value as T)}
      className={cx(
        "no-drag h-9 w-full cursor-pointer rounded-[8px] border",
        "border-[var(--border-default)] bg-[var(--bg-sunken)] px-3 text-[13px]",
        "text-[var(--text-primary)] transition-colors hover:border-[var(--border-strong)]",
        "disabled:opacity-40",
      )}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

/** Horizontal choice control for 2–4 short options. */
export function Segmented<T extends string>({
  value,
  onChange,
  options,
  disabled,
}: {
  value: T;
  onChange: (next: T) => void;
  options: Array<{ value: T; label: string }>;
  disabled?: boolean;
}) {
  return (
    <div
      role="radiogroup"
      className={cx(
        "no-drag inline-flex gap-0.5 rounded-[9px] border p-0.5",
        "border-[var(--border-subtle)] bg-[var(--bg-sunken)]",
        disabled && "pointer-events-none opacity-40",
      )}
    >
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            role="radio"
            aria-checked={active}
            onClick={() => onChange(o.value)}
            className={cx(
              "rounded-[7px] px-3 py-1 text-[12px] font-medium transition-all",
              "duration-[var(--dur-fast)]",
              active
                ? "bg-[var(--bg-overlay)] text-[var(--text-primary)] shadow-[var(--shadow-sm)]"
                : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/** Swatch picker used for widget tints and the taskbar colour. */
export function ColorSwatches({
  value,
  onChange,
  colors,
}: {
  value: number;
  onChange: (next: number) => void;
  colors: Array<{ value: number; label: string }>;
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {colors.map((c) => {
        const hex = `#${c.value.toString(16).padStart(6, "0")}`;
        const active = c.value === value;
        return (
          <button
            key={c.value}
            title={c.label}
            aria-label={c.label}
            aria-pressed={active}
            onClick={() => onChange(c.value)}
            className={cx(
              "no-drag h-7 w-7 rounded-[7px] border-2 transition-transform",
              "duration-[var(--dur-fast)] hover:scale-110",
              active ? "border-[var(--accent-400)]" : "border-[var(--border-default)]",
            )}
            style={{ background: hex }}
          />
        );
      })}
    </div>
  );
}

// --------------------------------------------------------------------------
// Tooltip
// --------------------------------------------------------------------------

export function Tooltip({
  label,
  children,
  side = "top",
}: {
  label: string;
  children: ReactNode;
  side?: "top" | "bottom" | "left" | "right";
}) {
  const [open, setOpen] = useState(false);
  const positions = {
    top: "bottom-full left-1/2 -translate-x-1/2 mb-1.5",
    bottom: "top-full left-1/2 -translate-x-1/2 mt-1.5",
    left: "right-full top-1/2 -translate-y-1/2 mr-1.5",
    right: "left-full top-1/2 -translate-y-1/2 ml-1.5",
  };
  return (
    <span
      className="relative inline-flex"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      {children}
      {open && (
        <span
          role="tooltip"
          className={cx(
            "animate-fade pointer-events-none absolute z-50 whitespace-nowrap",
            "rounded-[6px] border border-[var(--border-default)] bg-[var(--bg-overlay)]",
            "px-2 py-1 text-[11px] text-[var(--text-primary)] shadow-[var(--shadow-md)]",
            positions[side],
          )}
        >
          {label}
        </span>
      )}
    </span>
  );
}

// --------------------------------------------------------------------------
// Modal
// --------------------------------------------------------------------------

export function Modal({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 460,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children?: ReactNode;
  footer?: ReactNode;
  width?: number;
}) {
  const titleId = useId();

  // Escape closes. Bound while open only, so a stack of modals does not
  // accumulate listeners.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="animate-fade fixed inset-0 z-[100] flex items-center justify-center p-6"
      style={{ background: "rgb(0 0 0 / 0.5)", backdropFilter: "blur(3px)" }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="animate-rise card max-h-full overflow-hidden shadow-[var(--shadow-lg)]"
        style={{ width, background: "var(--bg-overlay)" }}
      >
        <div className="border-b border-[var(--border-subtle)] px-5 py-4">
          <h3 id={titleId} className="text-[14px] font-semibold">
            {title}
          </h3>
          {description && (
            <p className="mt-1 text-[12px] leading-relaxed text-[var(--text-secondary)]">
              {description}
            </p>
          )}
        </div>
        {children && (
          <div className="max-h-[52vh] overflow-y-auto px-5 py-4">{children}</div>
        )}
        {footer && (
          <div className="flex justify-end gap-2 border-t border-[var(--border-subtle)] px-5 py-3.5">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}

export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  description,
  confirmLabel = "Confirm",
  destructive,
  children,
  busy,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  description?: string;
  confirmLabel?: string;
  destructive?: boolean;
  children?: ReactNode;
  busy?: boolean;
}) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      description={description}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button
            variant={destructive ? "danger" : "primary"}
            onClick={onConfirm}
            loading={busy}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      {children}
    </Modal>
  );
}

// --------------------------------------------------------------------------
// Empty and loading states
// --------------------------------------------------------------------------

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center px-6 py-14 text-center">
      {icon && <div className="mb-3 text-[var(--text-tertiary)]">{icon}</div>}
      <p className="text-[13px] font-medium text-[var(--text-secondary)]">{title}</p>
      {description && (
        <p className="mt-1 max-w-[42ch] text-[12px] leading-relaxed text-[var(--text-tertiary)]">
          {description}
        </p>
      )}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cx("skeleton", className)} />;
}

// --------------------------------------------------------------------------
// Toasts
// --------------------------------------------------------------------------

export interface Toast {
  id: number;
  message: string;
  tone: Tone;
  detail?: string;
}

interface ToastApi {
  push: (message: string, tone?: Tone, detail?: string) => void;
}

const ToastContext = createContext<ToastApi>({ push: () => {} });

export const useToast = () => useContext(ToastContext);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const next = useRef(1);

  const push = useCallback((message: string, tone: Tone = "neutral", detail?: string) => {
    const id = next.current++;
    setToasts((t) => [...t.slice(-3), { id, message, tone, detail }]);
    // Errors stay longer: they carry more to read, and reading them is the
    // whole point.
    const ms = tone === "danger" ? 8000 : 4000;
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), ms);
  }, []);

  const api = useMemo(() => ({ push }), [push]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="pointer-events-none fixed bottom-4 right-4 z-[200] flex w-[340px] flex-col gap-2">
        {toasts.map((t) => (
          <div
            key={t.id}
            role="status"
            className="animate-rise card pointer-events-auto flex gap-2.5 px-3.5 py-3 shadow-[var(--shadow-lg)]"
            style={{ background: "var(--bg-overlay)" }}
          >
            <span className="mt-[5px]">
              <Dot tone={t.tone} />
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-[12.5px] leading-snug text-[var(--text-primary)]">
                {t.message}
              </p>
              {t.detail && (
                <p className="mt-1 text-[11px] leading-snug text-[var(--text-tertiary)]">
                  {t.detail}
                </p>
              )}
            </div>
            <button
              aria-label="Dismiss"
              onClick={() => setToasts((x) => x.filter((v) => v.id !== t.id))}
              className="h-4 w-4 shrink-0 text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
            >
              <CloseIcon />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

// --------------------------------------------------------------------------
// Icons
//
// Inline so there is no icon-font dependency and every glyph inherits
// currentColor. 16px grid, 1.5 stroke, round caps — one visual family.
// --------------------------------------------------------------------------

type IconProps = { size?: number; className?: string };

const svg = (size: number, className?: string) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  className,
  "aria-hidden": true,
});

export const HomeIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M3 10.5 12 3l9 7.5" />
    <path d="M5 9.5V20h14V9.5" />
  </svg>
);

export const SlidersIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
    <circle cx="16" cy="7" r="2" />
    <circle cx="10" cy="17" r="2" />
  </svg>
);

export const LayersIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="m12 3 9 5-9 5-9-5 9-5Z" />
    <path d="m3 13 9 5 9-5" />
  </svg>
);

export const PlugIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M9 3v6M15 3v6" />
    <path d="M6 9h12v3a6 6 0 0 1-12 0V9Z" />
    <path d="M12 18v3" />
  </svg>
);

export const GearIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9 7 7M17 17l2.1 2.1M19.1 4.9 17 7M7 17l-2.1 2.1" />
  </svg>
);

export const RestoreIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M3 12a9 9 0 1 0 2.6-6.4" />
    <path d="M3 4v5h5" />
  </svg>
);

export const CloseIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M6 6l12 12M18 6 6 18" />
  </svg>
);

export const MinimiseIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M5 12h14" />
  </svg>
);

export const MaximiseIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <rect x="5" y="5" width="14" height="14" rx="2" />
  </svg>
);

export const PlusIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const CheckIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="m4 12.5 5 5L20 6.5" />
  </svg>
);

export const AlertIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M12 4 2.5 20h19L12 4Z" />
    <path d="M12 10v4M12 17.5v.01" />
  </svg>
);

export const InfoIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v5M12 8v.01" />
  </svg>
);

export const ChevronIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="m9 6 6 6-6 6" />
  </svg>
);

export const ExternalIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M14 4h6v6" />
    <path d="M20 4 10 14" />
    <path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
  </svg>
);

export const TrashIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13" />
  </svg>
);

export const CopyIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15V5a1 1 0 0 1 1-1h9" />
  </svg>
);

export const DownloadIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M12 4v11M7.5 10.5 12 15l4.5-4.5" />
    <path d="M4 18h16" />
  </svg>
);

export const UploadIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M12 16V5M7.5 9.5 12 5l4.5 4.5" />
    <path d="M4 19h16" />
  </svg>
);

export const ClockIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3.5 2" />
  </svg>
);

export const MonitorIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <rect x="3" y="4" width="18" height="12" rx="2" />
    <path d="M8 20h8M12 16v4" />
  </svg>
);

export const SparkIcon = ({ size = 16, className }: IconProps) => (
  <svg {...svg(size, className)}>
    <path d="M12 3v4M12 17v4M3 12h4M17 12h4" />
    <path d="M12 8.5 13.2 11l2.8 1-2.8 1-1.2 2.5L10.8 13 8 12l2.8-1L12 8.5Z" />
  </svg>
);
