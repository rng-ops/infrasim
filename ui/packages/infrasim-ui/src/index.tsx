import React, { PropsWithChildren, forwardRef, useEffect, useId, useMemo, useRef, useState, useCallback } from "react";
import clsx from "clsx";
import "./tokens.css";

// ============================================================================
// Button
// ============================================================================

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export const Button = forwardRef<HTMLButtonElement, PropsWithChildren<{ variant?: ButtonVariant; onClick?: () => void; type?: "button" | "submit" | "reset"; className?: string; disabled?: boolean; "aria-label"?: string; loading?: boolean; size?: "sm" | "md" | "lg"; }>>(function Button(
  { variant = "primary", children, className, loading, size = "md", disabled, ...rest },
  ref
) {
  return (
    <button
      ref={ref}
      className={clsx(
        "ifm-btn",
        `ifm-btn-${variant}`,
        size !== "md" && `ifm-btn-${size}`,
        loading && "ifm-btn-loading",
        className
      )}
      disabled={disabled || loading}
      {...rest}
    >
      {loading && <span className="ifm-btn-spinner" aria-hidden />}
      {children}
    </button>
  );
});

// ============================================================================
// Card
// ============================================================================

export const Card: React.FC<PropsWithChildren<{ title?: string; actions?: React.ReactNode; className?: string; padding?: "none" | "sm" | "md" | "lg" }>> = ({ title, actions, className, padding = "md", children }) => (
  <section className={clsx("ifm-card", `ifm-card-padding-${padding}`, className)}>
    {(title || actions) && (
      <div className="ifm-card__header">
        {title && <h3>{title}</h3>}
        {actions && <div className="ifm-card__actions">{actions}</div>}
      </div>
    )}
    <div className="ifm-card__body">{children}</div>
  </section>
);

// ============================================================================
// StatusChip / StatusPill (material-based)
// ============================================================================

export type StatusTone = "success" | "danger" | "warning" | "info" | "muted";

export const StatusChip: React.FC<{ label: string; tone?: StatusTone; size?: "sm" | "md"; glow?: boolean }> = ({ label, tone = "info", size = "md", glow }) => (
  <span className={clsx("ifm-chip", `ifm-chip-${tone}`, size === "sm" && "ifm-chip-sm", glow && "ifm-chip-glow")}>{label}</span>
);

export const StatusPill = StatusChip; // Alias

// Health ring for appliance cards
export const HealthRing: React.FC<{ status: "healthy" | "degraded" | "unhealthy" | "unknown"; size?: number }> = ({ status, size = 12 }) => {
  const colors: Record<string, string> = {
    healthy: "#22c55e",
    degraded: "#eab308",
    unhealthy: "#ef4444",
    unknown: "#6b7280",
  };
  return (
    <span
      className="ifm-health-ring"
      style={{ width: size, height: size, backgroundColor: colors[status] || colors.unknown }}
      aria-label={`Status: ${status}`}
    />
  );
};

// ============================================================================
// Spinner
// ============================================================================

export const Spinner: React.FC<{ label?: string; size?: "sm" | "md" | "lg" }> = ({ label = "Loading", size = "md" }) => (
  <div className={clsx("ifm-spinner", `ifm-spinner-${size}`)} role="status" aria-live="polite">
    <div className="ifm-spinner__dot" />
    <span className="sr-only">{label}</span>
  </div>
);

// ============================================================================
// PageHeader
// ============================================================================

export const PageHeader: React.FC<{ title: string; description?: string; breadcrumbs?: React.ReactNode; actions?: React.ReactNode; }> = ({ title, description, breadcrumbs, actions }) => (
  <header className="ifm-page-header" aria-labelledby="page-title">
    {breadcrumbs && <nav aria-label="Breadcrumb">{breadcrumbs}</nav>}
    <div className="ifm-page-header__main">
      <div>
        <h1 id="page-title">{title}</h1>
        {description && <p className="ifm-page-header__desc">{description}</p>}
      </div>
      {actions && <div className="ifm-page-header__actions">{actions}</div>}
    </div>
  </header>
);

// ============================================================================
// Breadcrumbs
// ============================================================================

export const Breadcrumbs: React.FC<{ items: Array<{ label: string; href?: string; onClick?: () => void }> }> = ({ items }) => (
  <ol className="ifm-breadcrumbs" aria-label="Breadcrumb">
    {items.map((item, i) => (
      <li key={i} className="ifm-breadcrumb-item">
        {i < items.length - 1 ? (
          item.href ? (
            <a href={item.href}>{item.label}</a>
          ) : item.onClick ? (
            <button type="button" onClick={item.onClick}>{item.label}</button>
          ) : (
            <span>{item.label}</span>
          )
        ) : (
          <span aria-current="page">{item.label}</span>
        )}
        {i < items.length - 1 && <span className="ifm-breadcrumb-sep" aria-hidden>/</span>}
      </li>
    ))}
  </ol>
);

// ============================================================================
// SkipLink
// ============================================================================

export const SkipLink: React.FC = () => (
  <a className="ifm-skip" href="#main" aria-label="Skip to main content">Skip to content</a>
);

// ============================================================================
// CodeBlock
// ============================================================================

export const CodeBlock: React.FC<{ code: string; onCopy?: () => void; label?: string; language?: string }> = ({ code, onCopy, label = "Code", language }) => (
  <div className="ifm-codeblock" aria-label={label} data-language={language}>
    <pre><code>{code}</code></pre>
    {onCopy && <Button variant="ghost" onClick={onCopy} aria-label="Copy code" size="sm">Copy</Button>}
  </div>
);

// ============================================================================
// DesignSystemStyles (injects global CSS)
// ============================================================================

export function DesignSystemStyles() {
  useEffect(() => {
    if (typeof document === "undefined") return;
    const existing = document.getElementById("ifm-ui-inline-styles");
    if (existing) return;
    const style = document.createElement("style");
    style.id = "ifm-ui-inline-styles";
    style.innerHTML = `
  /* Base button */
  .ifm-btn { font-family: var(--ifm-font-body); border: 1px solid transparent; border-radius: var(--ifm-radius-md); padding: 10px 14px; cursor: pointer; color: var(--ifm-color-text); background: var(--ifm-color-accent-strong); transition: transform 120ms ease, box-shadow 120ms ease; display: inline-flex; align-items: center; gap: 8px; font-size: 14px; font-weight: 500; }
  .ifm-btn:hover:not(:disabled) { transform: translateY(-1px); box-shadow: 0 12px 30px rgba(0,0,0,0.35); }
  .ifm-btn:focus-visible { outline: 2px solid #fff; outline-offset: 3px; }
  .ifm-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .ifm-btn-secondary { background: transparent; border-color: var(--ifm-color-border); }
  .ifm-btn-ghost { background: transparent; color: var(--ifm-color-text); padding: 6px 10px; }
  .ifm-btn-danger { background: var(--ifm-color-danger); }
  .ifm-btn-sm { padding: 6px 10px; font-size: 13px; }
  .ifm-btn-lg { padding: 14px 20px; font-size: 16px; }
  .ifm-btn-loading { pointer-events: none; }
  .ifm-btn-spinner { width: 14px; height: 14px; border: 2px solid currentColor; border-top-color: transparent; border-radius: 50%; animation: ifm-spin 0.6s linear infinite; }
  @keyframes ifm-spin { to { transform: rotate(360deg); } }

  /* Card */
  .ifm-card { background: var(--ifm-color-card); border: 1px solid var(--ifm-color-border); border-radius: var(--ifm-radius-lg); box-shadow: var(--ifm-shadow-card); color: var(--ifm-color-text); }
  .ifm-card-padding-none { padding: 0; }
  .ifm-card-padding-sm { padding: var(--ifm-space-3); }
  .ifm-card-padding-md { padding: var(--ifm-space-5); }
  .ifm-card-padding-lg { padding: var(--ifm-space-7); }
  .ifm-card__header { display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--ifm-space-3); }
  .ifm-card__body { color: var(--ifm-color-subtle); }

  /* Chips */
  .ifm-chip { display: inline-flex; align-items: center; padding: 4px 10px; border-radius: 999px; font-size: 12px; font-weight: 500; }
  .ifm-chip-sm { padding: 2px 6px; font-size: 11px; }
  .ifm-chip-success { background: rgba(34,197,94,0.15); color: #22c55e; }
  .ifm-chip-danger { background: rgba(239,68,68,0.15); color: #ef4444; }
  .ifm-chip-warning { background: rgba(234,179,8,0.15); color: #eab308; }
  .ifm-chip-info { background: rgba(96,165,250,0.15); color: #60a5fa; }
  .ifm-chip-muted { background: rgba(148,163,184,0.15); color: #cbd5e1; }
  .ifm-chip-glow { box-shadow: 0 0 8px currentColor; }

  /* Health ring */
  .ifm-health-ring { display: inline-block; border-radius: 50%; flex-shrink: 0; }

  /* Spinner */
  .ifm-spinner { display: inline-flex; align-items: center; gap: 8px; }
  .ifm-spinner__dot { width: 12px; height: 12px; border-radius: 50%; background: var(--ifm-color-accent); animation: ifm-pulse 1s infinite ease-in-out; }
  .ifm-spinner-sm .ifm-spinner__dot { width: 8px; height: 8px; }
  .ifm-spinner-lg .ifm-spinner__dot { width: 20px; height: 20px; }
  @keyframes ifm-pulse { 0% { transform: scale(1); opacity: 1; } 50% { transform: scale(1.3); opacity: 0.6; } 100% { transform: scale(1); opacity: 1; } }

  /* Page header */
  .ifm-page-header { margin-bottom: var(--ifm-space-6); }
  .ifm-page-header__main { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--ifm-space-4); flex-wrap: wrap; }
  .ifm-page-header__desc { color: var(--ifm-color-subtle); margin-top: var(--ifm-space-2); }

  /* Breadcrumbs */
  .ifm-breadcrumbs { display: flex; align-items: center; gap: 4px; list-style: none; padding: 0; margin: 0 0 var(--ifm-space-3); font-size: 13px; }
  .ifm-breadcrumb-item { display: flex; align-items: center; gap: 4px; }
  .ifm-breadcrumb-item a, .ifm-breadcrumb-item button { color: var(--ifm-color-accent); background: none; border: none; cursor: pointer; text-decoration: none; }
  .ifm-breadcrumb-item a:hover, .ifm-breadcrumb-item button:hover { text-decoration: underline; }
  .ifm-breadcrumb-sep { color: var(--ifm-color-subtle); }

  /* Skip link */
  .ifm-skip { position: absolute; left: -999px; top: -999px; background: #fff; color: #000; padding: 8px 12px; z-index: 999; }
  .ifm-skip:focus { left: 12px; top: 12px; }

  /* Codeblock */
  .ifm-codeblock { position: relative; border-radius: var(--ifm-radius-md); background: #0b1020; padding: var(--ifm-space-4); border: 1px solid var(--ifm-color-border); }
  .ifm-codeblock pre { margin: 0; color: var(--ifm-color-text); overflow-x: auto; }
  .ifm-codeblock .ifm-btn { position: absolute; top: 8px; right: 8px; }

  /* Table */
  .ifm-table { width: 100%; border-collapse: collapse; }
  .ifm-table th, .ifm-table td { text-align: left; padding: 10px 12px; border-top: 1px solid var(--ifm-color-border); vertical-align: top; }
  .ifm-table thead th { color: var(--ifm-color-text); border-top: 0; font-weight: 600; font-size: 13px; text-transform: uppercase; letter-spacing: 0.5px; }
  .ifm-table tbody tr:hover { background: rgba(255,255,255,0.03); }
  .ifm-table__nowrap { white-space: nowrap; }
  .ifm-table__num { text-align: right; font-variant-numeric: tabular-nums; }
  .ifm-table-sortable th { cursor: pointer; user-select: none; }
  .ifm-table-sortable th:hover { color: var(--ifm-color-accent); }
  .ifm-table__checkbox { width: 40px; text-align: center; }

  /* Tabs */
  .ifm-tabs { display: flex; gap: 8px; border-bottom: 1px solid var(--ifm-color-border); padding-bottom: 8px; overflow-x: auto; }
  .ifm-tab { background: transparent; border: 1px solid transparent; color: var(--ifm-color-subtle); padding: 8px 12px; border-radius: 999px; cursor: pointer; white-space: nowrap; font-size: 14px; }
  .ifm-tab[aria-selected="true"] { color: var(--ifm-color-text); border-color: var(--ifm-color-border); background: rgba(255,255,255,0.04); }
  .ifm-tab:focus-visible { outline: 2px solid #fff; outline-offset: 3px; }
  .ifm-tabpanel { padding-top: 16px; }

  /* Dialog */
  .ifm-dialog__backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.55); display: grid; place-items: center; z-index: 9999; }
  .ifm-dialog { width: min(720px, calc(100vw - 32px)); max-height: min(80vh, 720px); overflow: auto; background: var(--ifm-color-card); border: 1px solid var(--ifm-color-border); border-radius: var(--ifm-radius-lg); box-shadow: 0 30px 90px rgba(0,0,0,0.5); padding: 18px; }
  .ifm-dialog-sm { width: min(480px, calc(100vw - 32px)); }
  .ifm-dialog-lg { width: min(960px, calc(100vw - 32px)); }
  .ifm-dialog__header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; margin-bottom: 12px; }
  .ifm-dialog__title { margin: 0; }
  .ifm-dialog__body { color: var(--ifm-color-subtle); }
  .ifm-dialog__footer { display: flex; justify-content: flex-end; gap: 10px; margin-top: 14px; }

  /* Error summary */
  .ifm-errorsummary { border: 1px solid rgba(239,68,68,0.35); background: rgba(239,68,68,0.10); border-radius: var(--ifm-radius-lg); padding: 14px; margin-bottom: 16px; }
  .ifm-errorsummary h2 { margin: 0 0 8px; font-size: 16px; }
  .ifm-errorsummary ul { margin: 0; padding-left: 18px; }
  .ifm-errorsummary a { color: #fff; text-decoration: underline; }

  /* Empty state */
  .ifm-empty { border: 1px dashed var(--ifm-color-border); border-radius: var(--ifm-radius-lg); padding: 32px; color: var(--ifm-color-subtle); text-align: center; }
  .ifm-empty h3 { margin: 0 0 8px; color: var(--ifm-color-text); }

  /* Toast region */
  .ifm-toast-region { position: fixed; right: 12px; bottom: 12px; display: grid; gap: 10px; z-index: 10000; }
  .ifm-toast { width: min(420px, calc(100vw - 24px)); background: rgba(17, 24, 39, 0.92); border: 1px solid var(--ifm-color-border); border-radius: var(--ifm-radius-lg); padding: 12px 14px; box-shadow: 0 20px 60px rgba(0,0,0,0.5); }
  .ifm-toast__title { margin: 0 0 6px; font-size: 14px; }
  .ifm-toast__desc { margin: 0; color: var(--ifm-color-subtle); font-size: 13px; }

  /* Input */
  .ifm-input { background: var(--ifm-color-surface); border: 1px solid var(--ifm-color-border); border-radius: var(--ifm-radius-md); padding: 10px 12px; color: var(--ifm-color-text); font-size: 14px; width: 100%; }
  .ifm-input:focus { outline: none; border-color: var(--ifm-color-accent); }
  .ifm-input::placeholder { color: var(--ifm-color-subtle); }
  .ifm-input-error { border-color: var(--ifm-color-danger); }

  /* Select */
  .ifm-select { background: var(--ifm-color-surface); border: 1px solid var(--ifm-color-border); border-radius: var(--ifm-radius-md); padding: 10px 32px 10px 12px; color: var(--ifm-color-text); font-size: 14px; appearance: none; background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%239ca3af' d='M2 4l4 4 4-4'/%3E%3C/svg%3E"); background-repeat: no-repeat; background-position: right 10px center; cursor: pointer; }
  .ifm-select:focus { outline: none; border-color: var(--ifm-color-accent); }

  /* Checkbox */
  .ifm-checkbox { display: inline-flex; align-items: center; gap: 8px; cursor: pointer; }
  .ifm-checkbox input { width: 16px; height: 16px; accent-color: var(--ifm-color-accent); cursor: pointer; }

  /* Form field */
  .ifm-field { margin-bottom: 16px; }
  .ifm-field__label { display: block; margin-bottom: 6px; font-size: 14px; font-weight: 500; }
  .ifm-field__hint { font-size: 12px; color: var(--ifm-color-subtle); margin-top: 4px; }
  .ifm-field__error { font-size: 12px; color: var(--ifm-color-danger); margin-top: 4px; }

  /* Dock layout */
  .ifm-dock { display: grid; grid-template-columns: auto 1fr auto; grid-template-rows: auto 1fr auto; height: 100%; gap: 0; }
  .ifm-dock__left { grid-area: 1 / 1 / 3 / 2; border-right: 1px solid var(--ifm-color-border); overflow: auto; }
  .ifm-dock__center { grid-area: 1 / 2 / 3 / 3; overflow: auto; }
  .ifm-dock__right { grid-area: 1 / 3 / 3 / 4; border-left: 1px solid var(--ifm-color-border); overflow: auto; }
  .ifm-dock__bottom { grid-area: 3 / 1 / 4 / 4; border-top: 1px solid var(--ifm-color-border); overflow: auto; }

  /* Panel */
  .ifm-panel { background: var(--ifm-color-surface); height: 100%; }
  .ifm-panel__header { padding: 12px 16px; border-bottom: 1px solid var(--ifm-color-border); display: flex; justify-content: space-between; align-items: center; }
  .ifm-panel__title { margin: 0; font-size: 14px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; }
  .ifm-panel__content { padding: 12px 16px; overflow: auto; }

  /* Property grid (details dock) */
  .ifm-propgrid { display: grid; gap: 0; }
  .ifm-propgrid__row { display: grid; grid-template-columns: 140px 1fr; gap: 8px; padding: 8px 0; border-bottom: 1px solid var(--ifm-color-border); }
  .ifm-propgrid__row:last-child { border-bottom: 0; }
  .ifm-propgrid__label { color: var(--ifm-color-subtle); font-size: 13px; }
  .ifm-propgrid__value { color: var(--ifm-color-text); font-size: 13px; word-break: break-all; }

  /* Toolbar */
  .ifm-toolbar { display: flex; align-items: center; gap: 8px; padding: 8px 12px; background: var(--ifm-color-surface); border-bottom: 1px solid var(--ifm-color-border); flex-wrap: wrap; }
  .ifm-toolbar__spacer { flex: 1; }
  .ifm-toolbar__divider { width: 1px; height: 24px; background: var(--ifm-color-border); }

  /* Search input */
  .ifm-search { position: relative; }
  .ifm-search input { padding-left: 36px; }
  .ifm-search__icon { position: absolute; left: 12px; top: 50%; transform: translateY(-50%); color: var(--ifm-color-subtle); pointer-events: none; }

  /* Filter chips bar */
  .ifm-filter-bar { display: flex; align-items: center; gap: 8px; padding: 8px 0; flex-wrap: wrap; }
  .ifm-filter-chip { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 999px; font-size: 12px; background: var(--ifm-color-surface); border: 1px solid var(--ifm-color-border); cursor: pointer; }
  .ifm-filter-chip[aria-pressed="true"] { background: var(--ifm-color-accent); border-color: var(--ifm-color-accent); color: #fff; }
  .ifm-filter-chip__remove { background: none; border: none; color: inherit; cursor: pointer; padding: 0; line-height: 1; }

  /* Bulk action bar */
  .ifm-bulk-bar { display: flex; align-items: center; gap: 12px; padding: 10px 16px; background: var(--ifm-color-accent-strong); border-radius: var(--ifm-radius-md); margin-bottom: 12px; }
  .ifm-bulk-bar__count { font-weight: 600; }
  .ifm-bulk-bar__actions { display: flex; gap: 8px; margin-left: auto; }

  /* Trust badge */
  .ifm-trust-badge { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: var(--ifm-radius-sm); font-size: 12px; font-weight: 500; }
  .ifm-trust-badge-local { background: rgba(148,163,184,0.15); color: #94a3b8; }
  .ifm-trust-badge-remote { background: rgba(234,179,8,0.15); color: #eab308; }
  .ifm-trust-badge-attested { background: rgba(34,197,94,0.15); color: #22c55e; }

  /* Progress bar */
  .ifm-progress { height: 8px; background: var(--ifm-color-border); border-radius: 999px; overflow: hidden; }
  .ifm-progress__bar { height: 100%; background: var(--ifm-color-accent); transition: width 300ms ease; }

  /* Timeline */
  .ifm-timeline { position: relative; padding-left: 24px; }
  .ifm-timeline::before { content: ''; position: absolute; left: 7px; top: 0; bottom: 0; width: 2px; background: var(--ifm-color-border); }
  .ifm-timeline__item { position: relative; padding-bottom: 16px; }
  .ifm-timeline__item:last-child { padding-bottom: 0; }
  .ifm-timeline__dot { position: absolute; left: -24px; top: 4px; width: 12px; height: 12px; border-radius: 50%; background: var(--ifm-color-accent); border: 2px solid var(--ifm-color-card); }
  .ifm-timeline__dot-success { background: var(--ifm-color-success); }
  .ifm-timeline__dot-danger { background: var(--ifm-color-danger); }
  .ifm-timeline__dot-muted { background: var(--ifm-color-subtle); }
  .ifm-timeline__time { font-size: 11px; color: var(--ifm-color-subtle); margin-bottom: 2px; }
  .ifm-timeline__title { font-size: 14px; font-weight: 500; }
  .ifm-timeline__desc { font-size: 13px; color: var(--ifm-color-subtle); margin-top: 2px; }

  /* Surface host (console/canvas) */
  .ifm-surface-host { position: relative; background: #000; border-radius: var(--ifm-radius-md); overflow: hidden; }
  .ifm-surface-host canvas { display: block; width: 100%; height: auto; }
  .ifm-surface-host__overlay { position: absolute; top: 8px; right: 8px; display: flex; gap: 4px; }
  .ifm-surface-host__connection { position: absolute; bottom: 8px; left: 8px; }

  /* sr-only */
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border-width: 0; }

  /* Diff list */
  .ifm-diff-list { }
  .ifm-diff-item { padding: 8px 0; border-bottom: 1px solid var(--ifm-color-border); }
  .ifm-diff-item:last-child { border-bottom: 0; }
  .ifm-diff-item__header { display: flex; align-items: center; gap: 8px; }
  .ifm-diff-item__type { font-size: 11px; font-weight: 600; text-transform: uppercase; padding: 2px 6px; border-radius: 4px; }
  .ifm-diff-item__type-add { background: rgba(34,197,94,0.15); color: #22c55e; }
  .ifm-diff-item__type-update { background: rgba(234,179,8,0.15); color: #eab308; }
  .ifm-diff-item__type-delete { background: rgba(239,68,68,0.15); color: #ef4444; }
  .ifm-diff-item__name { font-weight: 500; }
  .ifm-diff-item__changes { margin-top: 4px; font-size: 12px; color: var(--ifm-color-subtle); padding-left: 12px; }

  /* Stepper */
  .ifm-stepper { display: flex; gap: 8px; margin-bottom: 16px; }
  .ifm-step { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: var(--ifm-radius-md); font-size: 13px; }
  .ifm-step-pending { background: var(--ifm-color-surface); color: var(--ifm-color-subtle); }
  .ifm-step-running { background: rgba(96,165,250,0.15); color: var(--ifm-color-accent); }
  .ifm-step-completed { background: rgba(34,197,94,0.15); color: #22c55e; }
  .ifm-step-error { background: rgba(239,68,68,0.15); color: #ef4444; }
  .ifm-step__number { width: 20px; height: 20px; border-radius: 50%; background: currentColor; color: var(--ifm-color-card); display: flex; align-items: center; justify-content: center; font-size: 11px; font-weight: 600; }
    `;
    document.head.appendChild(style);
  }, []);
  return null;
}

export const Table: React.FC<PropsWithChildren<{ caption?: string; className?: string }>> = ({ caption, className, children }) => (
  <table className={clsx("ifm-table", className)}>
    {caption && <caption className="sr-only">{caption}</caption>}
    {children}
  </table>
);

type TabItem = { id: string; label: string; panel: React.ReactNode; disabled?: boolean };

export function Tabs({ items, initialId }: { items: TabItem[]; initialId?: string }) {
  const enabledItems = items.filter(i => !i.disabled);
  const firstId = enabledItems[0]?.id;
  const [activeId, setActiveId] = useState<string>(initialId ?? firstId ?? items[0]?.id ?? "");
  const tablistId = useId();
  const activeIndex = Math.max(0, items.findIndex(i => i.id === activeId));

  function move(delta: number) {
    if (!items.length) return;
    let i = activeIndex;
    for (let step = 0; step < items.length; step++) {
      i = (i + delta + items.length) % items.length;
      if (!items[i].disabled) {
        setActiveId(items[i].id);
        return;
      }
    }
  }

  return (
    <div>
      <div
        className="ifm-tabs"
        role="tablist"
        aria-label="Tabs"
        id={tablistId}
        onKeyDown={(e) => {
          if (e.key === "ArrowRight") { e.preventDefault(); move(1); }
          if (e.key === "ArrowLeft") { e.preventDefault(); move(-1); }
          if (e.key === "Home") { e.preventDefault(); setActiveId(firstId ?? items[0]?.id ?? ""); }
          if (e.key === "End") { e.preventDefault(); setActiveId(enabledItems[enabledItems.length - 1]?.id ?? items[items.length - 1]?.id ?? ""); }
        }}
      >
        {items.map((t) => (
          <button
            key={t.id}
            className="ifm-tab"
            role="tab"
            type="button"
            aria-selected={t.id === activeId}
            aria-controls={`${tablistId}-${t.id}-panel`}
            id={`${tablistId}-${t.id}-tab`}
            disabled={t.disabled}
            tabIndex={t.id === activeId ? 0 : -1}
            onClick={() => setActiveId(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      {items.map((t) => (
        <div
          key={t.id}
          className="ifm-tabpanel"
          role="tabpanel"
          hidden={t.id !== activeId}
          id={`${tablistId}-${t.id}-panel`}
          aria-labelledby={`${tablistId}-${t.id}-tab`}
          tabIndex={0}
        >
          {t.panel}
        </div>
      ))}
    </div>
  );
}

function useFocusTrap(active: boolean, containerRef: React.RefObject<HTMLElement>) {
  useEffect(() => {
    if (!active) return;
    const el = containerRef.current;
    if (!el) return;
    const elMounted = el;
    const focusableSelector = [
      'a[href]',
      'button:not([disabled])',
      'textarea:not([disabled])',
      'input:not([disabled])',
      'select:not([disabled])',
      '[tabindex]:not([tabindex="-1"])'
    ].join(',');

    const focusables = Array.from(elMounted.querySelectorAll<HTMLElement>(focusableSelector));
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    first?.focus();

    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== 'Tab') return;
      if (focusables.length === 0) return;
      const activeEl = document.activeElement as HTMLElement | null;
      if (e.shiftKey) {
        if (activeEl === first || !elMounted.contains(activeEl)) {
          e.preventDefault();
          last?.focus();
        }
      } else {
        if (activeEl === last) {
          e.preventDefault();
          first?.focus();
        }
      }
    }

    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [active, containerRef]);
}

export function Dialog({ open, title, description, onClose, footer, children }: PropsWithChildren<{ open: boolean; title: string; description?: string; onClose: () => void; footer?: React.ReactNode; }>) {
  const titleId = useId();
  const descId = useId();
  const containerRef = useRef<HTMLDivElement>(null);
  useFocusTrap(open, containerRef);

  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div className="ifm-dialog__backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div
        className="ifm-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descId : undefined}
        ref={containerRef}
      >
        <div className="ifm-dialog__header">
          <div>
            <h2 className="ifm-dialog__title" id={titleId}>{title}</h2>
            {description && <p id={descId} className="ifm-dialog__body">{description}</p>}
          </div>
          <Button variant="ghost" aria-label="Close dialog" onClick={onClose}>Close</Button>
        </div>
        <div className="ifm-dialog__body">{children}</div>
        {footer && <div className="ifm-dialog__footer">{footer}</div>}
      </div>
    </div>
  );
}

export type ToastTone = "info" | "success" | "danger";
export type ToastItem = { id: string; title: string; description?: string; tone?: ToastTone; createdAtMs: number };

export function useToasts() {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  function push(t: Omit<ToastItem, "id" | "createdAtMs"> & { id?: string }) {
    const id = t.id ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const toast: ToastItem = { id, title: t.title, description: t.description, tone: t.tone, createdAtMs: Date.now() };
    setToasts(prev => [toast, ...prev].slice(0, 4));
    return id;
  }
  function dismiss(id: string) {
    setToasts(prev => prev.filter(t => t.id !== id));
  }
  return { toasts, push, dismiss };
}

export function ToastRegion({ toasts, onDismiss }: { toasts: ToastItem[]; onDismiss: (id: string) => void }) {
  return (
    <div className="ifm-toast-region" aria-live="polite" aria-relevant="additions removals">
      {toasts.map(t => (
        <div key={t.id} className="ifm-toast" role="status">
          <div style={{ display: "flex", justifyContent: "space-between", gap: 10, alignItems: "flex-start" }}>
            <div>
              <h3 className="ifm-toast__title">{t.title}</h3>
              {t.description && <p className="ifm-toast__desc">{t.description}</p>}
            </div>
            <Button variant="ghost" aria-label="Dismiss toast" onClick={() => onDismiss(t.id)}>Dismiss</Button>
          </div>
        </div>
      ))}
    </div>
  );
}

export function ErrorSummary({ title = "There is a problem", errors }: { title?: string; errors: Array<{ message: string; href?: string }> }) {
  const id = useId();
  useEffect(() => {
    if (typeof document === "undefined") return;
    const el = document.getElementById(id);
    if (el && el instanceof HTMLElement) el.focus();
  }, [id]);

  if (!errors.length) return null;
  return (
    <section
      className="ifm-errorsummary"
      role="alert"
      aria-labelledby={`${id}-title`}
      tabIndex={-1}
      id={id}
    >
      <h2 id={`${id}-title`}>{title}</h2>
      <ul>
        {errors.map((e, idx) => (
          <li key={idx}>
            {e.href ? <a href={e.href}>{e.message}</a> : e.message}
          </li>
        ))}
      </ul>
    </section>
  );
}

export function EmptyState({ title, description, actions }: { title: string; description?: string; actions?: React.ReactNode }) {
  return (
    <section className="ifm-empty" aria-label={title}>
      <h3 style={{ margin: 0 }}>{title}</h3>
      {description && <p style={{ marginTop: 8 }}>{description}</p>}
      {actions && <div style={{ marginTop: 12 }}>{actions}</div>}
    </section>
  );
}

export type Step = { id: string; label: string; content: React.ReactNode; validate?: () => string[] };
export function StepWizard({ steps, initialStepId, onFinish }: { steps: Step[]; initialStepId?: string; onFinish: () => void }) {
  const startId = initialStepId ?? steps[0]?.id;
  const [activeId, setActiveId] = useState(startId);
  const [errors, setErrors] = useState<string[]>([]);
  const idx = useMemo(() => Math.max(0, steps.findIndex(s => s.id === activeId)), [activeId, steps]);
  const step = steps[idx];

  function next() {
    const errs = step?.validate?.() ?? [];
    setErrors(errs);
    if (errs.length) return;
    if (idx >= steps.length - 1) onFinish();
    else setActiveId(steps[idx + 1].id);
  }
  function back() {
    setErrors([]);
    if (idx > 0) setActiveId(steps[idx - 1].id);
  }

  return (
    <div>
      {errors.length > 0 && <ErrorSummary errors={errors.map(m => ({ message: m }))} />}
      <p className="sr-only" aria-live="polite">Step {idx + 1} of {steps.length}: {step?.label}</p>
      <div className="ifm-tabs" role="list" aria-label="Steps">
        {steps.map((s, i) => (
          <div key={s.id} className="ifm-chip ifm-chip-muted" role="listitem" aria-current={s.id === activeId ? "step" : undefined}>
            {i + 1}. {s.label}
          </div>
        ))}
      </div>
      <div style={{ marginTop: 12 }}>{step?.content}</div>
      <div style={{ display: "flex", justifyContent: "space-between", gap: 12, marginTop: 16 }}>
        <Button variant="secondary" onClick={back} disabled={idx === 0}>Back</Button>
        <Button variant="primary" onClick={next}>{idx === steps.length - 1 ? "Finish" : "Next"}</Button>
      </div>
    </div>
  );
}

// ============================================================================
// Input Components
// ============================================================================

export const Input = forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement> & { error?: boolean }>(
  function Input({ className, error, ...props }, ref) {
    return <input ref={ref} className={clsx("ifm-input", error && "ifm-input-error", className)} {...props} />;
  }
);

export const Select = forwardRef<HTMLSelectElement, React.SelectHTMLAttributes<HTMLSelectElement>>(
  function Select({ className, children, ...props }, ref) {
    return <select ref={ref} className={clsx("ifm-select", className)} {...props}>{children}</select>;
  }
);

export const Checkbox: React.FC<{ checked: boolean; onChange: (checked: boolean) => void; label: string; disabled?: boolean }> = ({ checked, onChange, label, disabled }) => (
  <label className="ifm-checkbox">
    <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} disabled={disabled} />
    {label}
  </label>
);

export const FormField: React.FC<PropsWithChildren<{ label: string; hint?: string; error?: string; required?: boolean }>> = ({ label, hint, error, required, children }) => (
  <div className="ifm-field">
    <label className="ifm-field__label">{label}{required && <span style={{ color: "var(--ifm-color-danger)" }}> *</span>}</label>
    {children}
    {hint && !error && <div className="ifm-field__hint">{hint}</div>}
    {error && <div className="ifm-field__error">{error}</div>}
  </div>
);

// ============================================================================
// Search Input
// ============================================================================

export const SearchInput: React.FC<{ value: string; onChange: (value: string) => void; placeholder?: string }> = ({ value, onChange, placeholder = "Search..." }) => (
  <div className="ifm-search">
    <span className="ifm-search__icon" aria-hidden>🔍</span>
    <Input value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} aria-label="Search" />
  </div>
);

// ============================================================================
// Filter Chips
// ============================================================================

export type FilterChipProps = { label: string; active: boolean; onToggle: () => void; onRemove?: () => void };
export const FilterChip: React.FC<FilterChipProps> = ({ label, active, onToggle, onRemove }) => (
  <button type="button" className="ifm-filter-chip" aria-pressed={active} onClick={onToggle}>
    {label}
    {onRemove && active && (
      <span className="ifm-filter-chip__remove" onClick={(e) => { e.stopPropagation(); onRemove(); }} aria-label="Remove filter">×</span>
    )}
  </button>
);

export const FilterBar: React.FC<PropsWithChildren> = ({ children }) => <div className="ifm-filter-bar">{children}</div>;

// ============================================================================
// Bulk Action Bar
// ============================================================================

export const BulkActionBar: React.FC<{ count: number; onClear: () => void; children: React.ReactNode }> = ({ count, onClear, children }) => (
  <div className="ifm-bulk-bar">
    <span className="ifm-bulk-bar__count">{count} selected</span>
    <Button variant="ghost" size="sm" onClick={onClear}>Clear</Button>
    <div className="ifm-bulk-bar__actions">{children}</div>
  </div>
);

// ============================================================================
// Trust Badge
// ============================================================================

export type TrustLevel = "local" | "remote" | "attested";
export const TrustBadge: React.FC<{ level: TrustLevel; label?: string }> = ({ level, label }) => {
  const labels: Record<TrustLevel, string> = { local: "Local", remote: "Remote", attested: "Attested" };
  return <span className={clsx("ifm-trust-badge", `ifm-trust-badge-${level}`)}>{label ?? labels[level]}</span>;
};

// ============================================================================
// Progress Bar
// ============================================================================

export const ProgressBar: React.FC<{ value: number; max?: number; label?: string }> = ({ value, max = 100, label }) => {
  const pct = Math.min(100, Math.max(0, (value / max) * 100));
  return (
    <div className="ifm-progress" role="progressbar" aria-valuenow={value} aria-valuemax={max} aria-label={label}>
      <div className="ifm-progress__bar" style={{ width: `${pct}%` }} />
    </div>
  );
};

// ============================================================================
// Timeline
// ============================================================================

export type TimelineItem = { id: string; time: string; title: string; description?: string; status?: "success" | "danger" | "muted" };
export const Timeline: React.FC<{ items: TimelineItem[] }> = ({ items }) => (
  <div className="ifm-timeline">
    {items.map((item) => (
      <div key={item.id} className="ifm-timeline__item">
        <div className={clsx("ifm-timeline__dot", item.status && `ifm-timeline__dot-${item.status}`)} />
        <div className="ifm-timeline__time">{item.time}</div>
        <div className="ifm-timeline__title">{item.title}</div>
        {item.description && <div className="ifm-timeline__desc">{item.description}</div>}
      </div>
    ))}
  </div>
);

// ============================================================================
// Dock Layout (Unreal-like)
// ============================================================================

export const DockLayout: React.FC<{ left?: React.ReactNode; center: React.ReactNode; right?: React.ReactNode; bottom?: React.ReactNode }> = ({ left, center, right, bottom }) => (
  <div className="ifm-dock">
    {left && <aside className="ifm-dock__left">{left}</aside>}
    <main className="ifm-dock__center">{center}</main>
    {right && <aside className="ifm-dock__right">{right}</aside>}
    {bottom && <footer className="ifm-dock__bottom">{bottom}</footer>}
  </div>
);

// ============================================================================
// Panel (for dock sections)
// ============================================================================

export const Panel: React.FC<PropsWithChildren<{ title: string; actions?: React.ReactNode }>> = ({ title, actions, children }) => (
  <div className="ifm-panel">
    <div className="ifm-panel__header">
      <h4 className="ifm-panel__title">{title}</h4>
      {actions}
    </div>
    <div className="ifm-panel__content">{children}</div>
  </div>
);

// ============================================================================
// Property Grid (Details Dock)
// ============================================================================

export type PropertyRow = { label: string; value: React.ReactNode };
export const PropertyGrid: React.FC<{ rows: PropertyRow[] }> = ({ rows }) => (
  <div className="ifm-propgrid">
    {rows.map((row, i) => (
      <div key={i} className="ifm-propgrid__row">
        <div className="ifm-propgrid__label">{row.label}</div>
        <div className="ifm-propgrid__value">{row.value}</div>
      </div>
    ))}
  </div>
);

// ============================================================================
// Toolbar
// ============================================================================

export const Toolbar: React.FC<PropsWithChildren> = ({ children }) => <div className="ifm-toolbar">{children}</div>;
export const ToolbarSpacer: React.FC = () => <div className="ifm-toolbar__spacer" />;
export const ToolbarDivider: React.FC = () => <div className="ifm-toolbar__divider" />;

// ============================================================================
// Diff List (for Terraform plan)
// ============================================================================

export type DiffItem = { type: "add" | "update" | "delete"; name: string; resourceType: string; changes?: string[] };
export const DiffList: React.FC<{ items: DiffItem[] }> = ({ items }) => (
  <div className="ifm-diff-list">
    {items.map((item, i) => (
      <div key={i} className="ifm-diff-item">
        <div className="ifm-diff-item__header">
          <span className={clsx("ifm-diff-item__type", `ifm-diff-item__type-${item.type}`)}>{item.type}</span>
          <span className="ifm-diff-item__name">{item.name}</span>
          <span style={{ color: "var(--ifm-color-subtle)", fontSize: 12 }}>({item.resourceType})</span>
        </div>
        {item.changes && item.changes.length > 0 && (
          <ul className="ifm-diff-item__changes">
            {item.changes.map((c, j) => <li key={j}>{c}</li>)}
          </ul>
        )}
      </div>
    ))}
  </div>
);

// ============================================================================
// Stepper (restore/apply progress)
// ============================================================================

export type StepperItem = { id: string; label: string; status: "pending" | "running" | "completed" | "error" };
export const Stepper: React.FC<{ steps: StepperItem[] }> = ({ steps }) => (
  <div className="ifm-stepper">
    {steps.map((step, i) => (
      <div key={step.id} className={clsx("ifm-step", `ifm-step-${step.status}`)}>
        <span className="ifm-step__number">{i + 1}</span>
        {step.label}
      </div>
    ))}
  </div>
);

// ============================================================================
// Surface Host (for console/canvas rendering)
// ============================================================================

export const SurfaceHost = forwardRef<HTMLDivElement, PropsWithChildren<{ className?: string; connectionStatus?: React.ReactNode; overlayActions?: React.ReactNode }>>(
  function SurfaceHost({ className, connectionStatus, overlayActions, children }, ref) {
    return (
      <div ref={ref} className={clsx("ifm-surface-host", className)}>
        {children}
        {overlayActions && <div className="ifm-surface-host__overlay">{overlayActions}</div>}
        {connectionStatus && <div className="ifm-surface-host__connection">{connectionStatus}</div>}
      </div>
    );
  }
);

// ============================================================================
// Capability Gate (shows disabled state with reason)
// ============================================================================

export const CapabilityGate: React.FC<PropsWithChildren<{ allowed: boolean; reason?: string }>> = ({ allowed, reason, children }) => {
  if (allowed) return <>{children}</>;
  return (
    <div style={{ position: "relative", display: "inline-block" }} title={reason ?? "Not permitted"}>
      <div style={{ opacity: 0.5, pointerEvents: "none" }}>{children}</div>
    </div>
  );
};

// ============================================================================
// Confirm Dialog (specialized)
// ============================================================================

export const ConfirmDialog: React.FC<{
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "danger" | "warning" | "info";
  onConfirm: () => void;
  onCancel: () => void;
  loading?: boolean;
}> = ({ open, title, description, confirmLabel = "Confirm", cancelLabel = "Cancel", tone = "warning", onConfirm, onCancel, loading }) => (
  <Dialog
    open={open}
    title={title}
    description={description}
    onClose={onCancel}
    footer={
      <>
        <Button variant="secondary" onClick={onCancel} disabled={loading}>{cancelLabel}</Button>
        <Button variant={tone === "danger" ? "danger" : "primary"} onClick={onConfirm} loading={loading}>{confirmLabel}</Button>
      </>
    }
  />
);
