# Training Data Plan - 2500 Rows Minimum

## Current State
- **Recovered from OpenAI batch**: 4807 examples with thinking (truncated conversations)
- **Conversation templates**: data/conversations/*.jsonl (full multi-turn)
- **Lost**: Full training data with complete conversations + thinking

## Goal
2500 usable training rows for SmolLM3-3B fine-tuning

## Options

### Option A: Use recovered data as-is (FASTEST)
- We have 4807 rows with thinking
- Conversations are truncated but model can still learn patterns
- **Time**: 0 hours (ready now)
- **Quality**: Lower, but might work for demo

### Option B: Regenerate from templates + add thinking (BEST)
1. Run `cargo run --bin augment-data` → generates ~6000 examples
2. Submit to OpenAI batch API for thinking annotation
3. Wait ~2 hours for batch to complete
4. Merge thinking into training data
- **Time**: ~3-4 hours
- **Cost**: ~$12
- **Quality**: Good

### Option C: Regenerate without thinking (MEDIUM)
1. Run `cargo run --bin augment-data`
2. Train without thinking field
- **Time**: ~30 min
- **Quality**: Medium (no reasoning, just input→output)

## Decision
[ ] Option A - Use recovered data now
[ ] Option B - Regenerate + thinking ($12 more)
[ ] Option C - Regenerate without thinking

## Commands

```bash
# Regenerate training data from templates
cargo run --bin augment-data

# Check row count
wc -l data/training/augmented_dataset.jsonl

# Start training (after data is ready)
# TODO: add training command
```

## Files
- `data/training/training_data.jsonl` - recovered data (4807 rows, truncated)
- `data/training/augmented_dataset.jsonl` - regenerated (gitignored)
- `data/conversations/*.jsonl` - source templates
