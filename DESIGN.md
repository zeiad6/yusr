---
name: Yusr Impeccable Design Language
description: Neo-Andalusian Sovereign Islamic FinTech Design System. Deep emerald sanctuary ground, royal Damascus gold accents, and forensic ledger clarity with micro-elevation, GSAP fluid transitions, and tactile React Bits interactions.
version: 1.0.0

colors:
  # Sovereign Anchors
  emerald-sanctuary: "#08322f"
  emerald-deep: "#041b19"
  emerald-surface: "#0c4440"
  emerald-highlight: "#145954"
  gold-royal: "#e6c36f"
  gold-bright: "#f4d789"
  gold-dim: "#b59445"
  gold-glow: "rgba(230, 195, 111, 0.18)"

  # Text Hierarchy (IBM Plex Sans Arabic)
  text-pearl: "#ffffff"
  text-warm: "#f7f5ee"
  text-subtle: "#cad5d2"
  text-faint: "#879f9b"

  # Semantic States
  surplus-emerald: "#10b981"
  deficit-crimson: "#ef4444"
  quarantine-amber: "#f59e0b"
  chain-valid-cyan: "#06b6d4"

typography:
  font-family: "'IBM Plex Sans Arabic', -apple-system, BlinkMacSystemFont, sans-serif"
  numbers: "tabular-nums"
  headings:
    h1: "text-2xl lg:text-3xl font-bold tracking-tight text-white"
    h2: "text-xl lg:text-2xl font-bold text-white"
    h3: "text-lg font-semibold text-white"
  meta: "text-xs font-medium text-emerald-200/70"

motion:
  engine: "GSAP 3.x + Transitions.dev CSS Micro-interactions"
  durations:
    instant: "120ms"
    swift: "240ms"
    fluid: "400ms"
    stately: "650ms"
  easings:
    spring: "cubic-bezier(0.175, 0.885, 0.32, 1.275)"
    out-smooth: "cubic-bezier(0.16, 1, 0.3, 1)"
    in-out-luxury: "cubic-bezier(0.65, 0, 0.35, 1)"

guardrails:
  - "Never use harsh pure black or standard generic gray cards."
  - "Always format currency numbers with tabular numerals (`tabular-nums`) to prevent layout shifts."
  - "Every financial write operation must provide tactile feedback and explicit confirmation."
  - "Preserve strict dual-language RTL/LTR symmetry with zero horizontal scroll clipping."
---

# Yusr (يُسر) — Impeccable Design Language System

## 1. Philosophical Grounding
Yusr represents autonomous non-profit fund accounting governed by forensic cryptographic immutability (SHA-256 ledger chaining) and strict Islamic Sharia principles. The user interface embodies trust, permanence, discretion, and sovereign execution.

## 2. Visual Architecture
- **Layer 0 (Sanctuary Ground):** Deepest emerald lacquer (`#041b19`) reflecting stability and calmness.
- **Layer 1 (Card Elevation):** Raised translucent slate-emerald panels with subtle gold-patina hairline borders (`border border-[#e6c36f]/20`).
- **Layer 2 (Interactive Spotlight):** React Bits dynamic mouse-tracking radial spotlight illuminating active areas with refined warmth.
- **Layer 3 (Foreground Authority):** Pearl text and gleaming gold headers with transitions.dev animated shimmer reflections.

## 3. Micro-Interaction Rules
- Cards tilt with gentle distance dampening on mouse move (`transitions.dev card-tilt`).
- Numbers pop in with spring mechanics (`transitions.dev number-pop-in`).
- Important statistics animate sequentially with zero frame drops using GSAP counter interpolators.
