# Entropy Scanner

## Purpose
Flag tokens in the document with unusually high Shannon entropy — typical of API keys, access tokens, base64-encoded secrets, hex-encoded private keys. Pairs naturally with the PII Detector for "before I share this, what's risky in it?" workflows.

## Capabilities
- `read_document` — needs the body to scan.

## Actions
| action_id | display label | params | output variant |
|---|---|---|---|
| `scan` | Scan for High-Entropy Strings | JSON: `{"min_entropy": 4.5, "min_length": 20}` (both optional) | `StructuredData` |

## Sample input → output

`scan` on `"my key is sk-abc123XYZdef456GHIjkl789MNOpqr"` returns:
```json
{
  "findings": [
    {
      "start": 10,
      "end": 47,
      "sample": "sk-abc123XYZdef456GHIjkl789MNOpqr",
      "entropy": 5.12,
      "kind": "high_entropy"
    }
  ],
  "count": 1
}
```

## Complexity estimate
Small. Tokenize on whitespace, compute Shannon entropy per token, filter by threshold.

## Host-db extensions needed
None.

## Suggested implementation notes
- Shannon entropy formula: H = -Σ p(x) log₂ p(x) over the character distribution of the token.
- Default `min_entropy` 4.5: catches most base64/hex secrets. Below 4.0 = lots of normal text. Above 5.0 = misses some real secrets.
- Default `min_length` 20: filters out short noisy tokens (timestamps, hashes-of-hashes).
- Skip tokens that are pure numbers, pure URLs, pure file paths — false-positive prone.
- Bonus pass: flag known-prefix patterns (`sk-`, `ghp_`, `xoxb-`, `AKIA`, `-----BEGIN PRIVATE KEY-----`) regardless of entropy. Provider-specific catches that pure entropy misses.
- Like PII Detector, this is a *detection* skill. A companion "Secret Redactor" could reuse `detect()` the way the existing Redactor reuses pii_detector::detect.
