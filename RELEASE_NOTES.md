# Yusr (يُسر) v0.2.0 — Sovereign Offline Fund Accounting Suite

**Release Tag:** `v0.2.0`  
**Target Platform:** Windows 10/11 x64 (Self-Contained MSI Package)  
**License:** PolyForm Noncommercial 1.0.0 (with Patent Defense Grant) — *Commercial use strictly prohibited without prior written authorization from the developer.*

---

## 🌟 Executive Overview | نظرة عامة
**Yusr (يُسر)** is an enterprise-grade, autonomous fund accounting and management system engineered specifically for charities, non-profit organizations, endowment funds (Awqaf), and Zakat committees. Built on a strict 100% offline-first architecture, Yusr runs entirely app-local with zero mandatory cloud dependencies, providing sovereign data ownership and tamper-evident financial forensic integrity.

---

## 📦 What's Inside This Package | محتويات الحزمة المكتفية ذاتياً
This release is distributed as a single, self-contained Windows Installer (`.msi`). It bundles all required runtimes and services out of the box with zero external prerequisites:
1. **Embedded PostgreSQL 18.6.1 Cluster:** Runs as an isolated app-local process bound to private loopback with custom socket directories.
2. **Embedded Offline AI Advisor (`llama.cpp` + Qwen 2B GGUF):** Specialized non-profit fund accounting model running natively on CPU/GPU without needing Ollama, Docker, or external Python interpreters.
3. **Core Server Engine (Rust / Axum / Tokio):** Ultra-low latency backend enforcing double-entry bookkeeping, fund segregation, and cryptographic validation.
4. **Desktop Shell (Tauri v2 + WebView2):** High-performance desktop application interface.
5. **Modern Dual-Language Web Interface (React 18 + Vite + Tailwind):** Native RTL/LTR layouts with authentic IBM Plex Sans Arabic typography.

---

## 🧩 Two Installers, One Version | حزمتان بنفس الإصدار
Version `0.2.0` ships as two Windows installers. The application, database and features are identical; only the advisor model differs.

| | **Full** — `Yusr_0.2.0_x64_en-US.msi` | **Lite** — `Yusr-Lite_0.2.0_x64_en-US.msi` |
|---|---|---|
| Advisor model (Qwen 2B GGUF) | Bundled in the installer | **Not bundled** — downloaded from Settings |
| Installer size | Large (includes the model) | Much smaller |
| Internet needed | Never | Once, only to download the model (everything else stays offline) |
| Model download | — | **Download button** in Settings → Advisor; runs in the background while you keep working |
| Resume | — | A paused, dropped or interrupted download (even after closing the app) **resumes where it stopped** |
| Integrity | Verified SHA-256 at build time | The same pinned SHA-256 verified after download; a damaged file is discarded |
| Start / stop the local model | Always on | **Start / Stop button** in Settings → Advisor; a stop is remembered across restarts (frees RAM/CPU) |
| Model location | Installation folder | `%LOCALAPPDATA%\Yusr\models\advisor.gguf` |
| Build workflow | `desktop.yml` (unchanged) | `desktop-lite.yml` |

**بالعربية:**
- **الحزمة الكاملة:** تتضمن النموذج المحلي للمستشار وتعمل دون إنترنت إطلاقاً، والنموذج يعمل دائماً.
- **الحزمة الخفيفة (Lite):** نفس البرنامج دون النموذج، فحجمها أصغر بكثير. من **الإعدادات ← المستشار الذكي**:
  - زر **تحميل النموذج**: يعمل في الخلفية أثناء استخدام البرنامج، مع زر إيقاف مؤقت/استئناف، ويُستأنف التحميل من حيث توقف عند انقطاع الاتصال أو إغلاق البرنامج، ثم يُتحقق من بصمة SHA-256 ويُشغَّل النموذج تلقائياً.
  - زر **تشغيل/إيقاف النموذج المحلي**: لتحرير الذاكرة والمعالج عند عدم الحاجة، ويُحفظ الاختيار بعد إعادة التشغيل.
- **ثبّت حزمة واحدة فقط.** للتبديل بينهما أزل الحزمة المثبتة أولاً من «التطبيقات المثبتة» ثم ثبّت الأخرى؛ البيانات محفوظة في مجلد المستخدم ولا تُحذف.

> Install only one of the two. To switch, uninstall the current one first (your data in `%LOCALAPPDATA%\Yusr` is kept), then install the other.

---

## 🚀 Key Architectural Modules & Features | أبرز الميزات والوحدات المعمارية

### 1. Default Credentials & Granular Access Control
- **Default Username:** `admin` (with instant copy button & auto-fill)
- **Default Initial Password:** `admin12345` (with instant copy button & auto-fill)
- **Granular Role-Based Permissions:** Admin, Accountant, Social Worker, Auditor, and Viewer.

### 2. LAN Server Mode & Multi-User Collaboration
- Switch effortlessly between Standalone (`127.0.0.1`) and LAN Server Mode (`0.0.0.0`).
- Dynamic subnet routing (`pg_hba.conf`) and connection pooling designed for high-spec host hardware.
- Dedicated client configuration mode allowing remote network PCs to connect directly to the central host.

### 3. Forensic SHA-256 Hash-Chain Ledger
- Write-once, append-only accounting ledger. Posted transactions cannot be edited or erased; corrections occur strictly via reversing entries (*القيود العكسية*).
- Instant cryptographic verification of database state via `/audit`.

### 4. ZATCA Phase 2 Ready (Saudi Electronic Invoicing)
- Cryptographic stamps, ECDSA secp256k1 signatures, UBL 2.1 XML invoices, and sequential invoice hashing.

### 5. Automated Multi-Target Backup Engine
- Background snapshot worker running atomic `pg_dump` backups every configured interval.
- Cryptographic SHA-256 verification against live row counts.
- Concurrent routing to secondary local partition, auto-detected removable USB/HDD media, and optional client-side encrypted cloud sync.

### 6. Dynamic Zakat Nisab Sync & Manual Fallback
- Dynamic tracking of official Gold (85g) and Silver (595g) market prices.
- 1-click authenticated online sync with graceful offline fallback and historical rate archive.

### 7. Internet-Independent Asynchronous Gateway (Queue Sync)
- Staging table for incoming payment webhooks and bank feeds with cryptographic deduplication and idempotency guards.

### 8. Cryptographically Signed Donor Transparency Reports
- Export tamper-evident financial reports embedded with the latest SHA-256 ledger block hash and verifiable QR code.
- Dedicated auditor verification page (`/verify/report`).

### 9. Neo-Andalusian Sovereign Design Language
- UI micro-interactions powered by GSAP, Transitions.dev, and React Bits (`SpotlightCard`, `ShinyText`, `CountUp`, `MagneticButton`).
- Sidebar geometry mathematically fitted to terminate exactly 1cm after Settings with zero excess height.

---

## 🔒 Installation & Verification | التثبيت والتحقق
1. Download `Yusr_0.2.0_x64_en-US.msi` (full) or `Yusr-Lite_0.2.0_x64_en-US.msi` (lite) and its matching `.sha256` checksum file.
2. Verify the package integrity in PowerShell:
   ```powershell
   Get-FileHash .\Yusr_0.2.0_x64_en-US.msi -Algorithm SHA256
   Get-FileHash .\Yusr-Lite_0.2.0_x64_en-US.msi -Algorithm SHA256
   ```
3. Run the installer. Launch Yusr from the Desktop or Start Menu.
4. On the sign-in screen, click **تعبئة البيانات الافتراضية** (Auto-fill) to sign in with `admin` / `admin12345`.
5. **Lite only:** open **الإعدادات ← المستشار الذكي** (Settings → Advisor) and click **تحميل النموذج** (Download the model). The advisor starts by itself once the download is verified.
