# @infrasim/ui

Small design system for the InfraSim Console.

Goals:
- Consistent visual language (tokens + components)
- Accessibility-first defaults (keyboard, focus, landmarks, ARIA)
- Minimal dependency surface

## Usage

In your app entry:

```tsx
import { DesignSystemStyles } from "@infrasim/ui";

export function Root() {
  return (
    <>
      <DesignSystemStyles />
      {/* app */}
    </>
  );
}
```

## Components

- `Button`, `Card`, `StatusChip`, `Spinner`, `PageHeader`, `SkipLink`
- `Table` (adds accessible caption helper)
- `Tabs` (roving focus + left/right/home/end keyboard)
- `Dialog` (focus trap + Escape + backdrop click)
- `ErrorSummary` (DWP-style summary, focuses on mount)
- `StepWizard` (multi-step flow with validation)
- `ToastRegion` + `useToasts`
- `EmptyState`

## Accessibility notes

- Always provide a visible page `<h1>` (the `PageHeader` does this).
- Use `SkipLink` + `id="main"` on your main content.
- `Dialog` is modal: do not stack deeply; keep the focusable content inside.
- `ErrorSummary` is intended to be placed near the top of the form container.

## Security considerations

This package is intentionally UI-only:
- No network calls
- No token storage
- No evaluation of untrusted HTML

Avoid passing untrusted strings into components that render as HTML. All components here render text via React escaping.
