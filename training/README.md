# Yusr AI Advisor — Accounting Training & Model Pipeline

This directory provides the domain training dataset, fine-tuning scripts, and GGUF quantization workflow for the **Yusr Embedded AI Advisor**.

## Autonomous Local Runtime Architecture

The Yusr desktop suite bundles its AI engine directly into the application:
- **Zero Third-Party Middleware**: Does not require Ollama, LM Studio, Docker, or external Python interpreters.
- **Embedded Execution**: Managed natively by `yusr-launcher` via a bundled, isolated `llama-server` process bound exclusively to `127.0.0.1` on a dynamic loopback port.
- **Read-Only by Construction**: The model only analyzes aggregated read-only snapshots; it has no database write access and cannot perform financial alterations.

---

## Dataset Contents (`accounting_dataset.jsonl`)

The training dataset covers specific non-profit financial scenarios:
1. **Fund Accounting (محاسبة الصناديق)**:
   - Unrestricted General Fund (أصول عامة تشغيلية).
   - Temporarily Restricted Funds (صناديق مقيدة زمنياً أو غرضياً: إغاثة، مساجد، إطعام).
   - Permanently Restricted Endowments (أصول الأوقاف: حبس الأصل وتسبيل المنفعة).
2. **Double-Entry Journal Balancing (القيود المزدوجة)**:
   - Gap-free numbered donation vouchers.
   - Non-destructive audit trail: erroneous postings must be corrected via full reversing entries (قيد عكسي), never edited or deleted.
3. **Zakat Calculation & Sharia Fiqh (فقه وحسابات الزكاة)**:
   - Gold & silver nisab thresholds (85g gold / 595g silver).
   - Tradable inventory, liquid cash, deductible current liabilities.
   - Strict allocation to the eight Quranic categories (مصارف الزكاة الثمانية).
   - Explicit prohibition of using Zakat for administrative overhead or capital expenditures.
4. **Voucher Disbursement & Aid Controls**:
   - Single-use cryptographically signed vouchers to prevent double disbursement.
   - Emergency case prioritization and score tracking.
5. **ZATCA Phase 2 E-Invoicing**:
   - Cryptographic stamp, ECDSA secp256k1 signature, SHA-256 hash chains, UBL 2.1 XML.

---

## Fine-Tuning Instructions

### 1. Requirements
```bash
pip install torch transformers datasets peft accelerate
```

### 2. Run LoRA Training
```bash
python3 training/finetune_lora.py \
  --base_model Qwen/Qwen2.5-3B-Instruct \
  --dataset_path training/accounting_dataset.jsonl \
  --output_dir training/lora_adapter \
  --epochs 3
```

### 3. Merge & Quantize to GGUF
```bash
./training/convert_and_quantize.sh Qwen/Qwen2.5-3B-Instruct training/lora_adapter
```
The resulting `advisor.gguf` will be copied directly to `app/resources/models/advisor.gguf` and packaged into the MSI installer.
