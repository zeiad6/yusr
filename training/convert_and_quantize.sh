#!/usr/bin/env bash
set -euo pipefail

# Convert fine-tuned LoRA weights, merge with base model, and quantize to GGUF (Q4_K_M)
# for direct embedding into Yusr Desktop (app/resources/models/advisor.gguf).

BASE_MODEL="${1:-Qwen/Qwen2.5-3B-Instruct}"
LORA_PATH="${2:-training/lora_adapter}"
MERGED_DIR="training/merged_model"
GGUF_F16="training/advisor_f16.gguf"
OUTPUT_GGUF="app/resources/models/advisor.gguf"

echo "=== 1. Merging LoRA adapter into base model weights ==="
python3 - << PYEOF
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer
from peft import PeftModel

base_name = "${BASE_MODEL}"
lora_path = "${LORA_PATH}"
merged_path = "${MERGED_DIR}"

print(f"Loading base model {base_name}...")
base_model = AutoModelForCausalLM.from_pretrained(base_name, torch_dtype=torch.float16, device_map="cpu")
tokenizer = AutoTokenizer.from_pretrained(base_name)

print(f"Loading and merging LoRA from {lora_path}...")
model = PeftModel.from_pretrained(base_model, lora_path)
model = model.merge_and_unload()

print(f"Saving merged model to {merged_path}...")
model.save_pretrained(merged_path)
tokenizer.save_pretrained(merged_path)
print("Merge complete!")
PYEOF

echo "=== 2. Converting to GGUF ==="
if [ ! -f "llama.cpp/convert_hf_to_gguf.py" ]; then
    echo "Cloning llama.cpp conversion tools..."
    git clone --depth 1 https://github.com/ggerganov/llama.cpp.git llama.cpp
fi

python3 llama.cpp/convert_hf_to_gguf.py "${MERGED_DIR}" --outfile "${GGUF_F16}" --outtype f16

echo "=== 3. Quantizing to Q4_K_M ==="
mkdir -p "$(dirname "${OUTPUT_GGUF}")"
llama.cpp/llama-quantize "${GGUF_F16}" "${OUTPUT_GGUF}" Q4_K_M

echo "=== Model ready for packaging: ${OUTPUT_GGUF} ==="
ls -lh "${OUTPUT_GGUF}"
