#!/bin/bash
set -e
SCRATCH="/var/folders/dn/_prdhdnn5l53lb71bhtx_n5w0000gn/T/grok-goal-b8b5184c22e6/implementer"
STORE="$SCRATCH/e2e-store"
ID=00ac1fb02ebfed8bc92bad6f9b85517c
C1=claim-1aad3375a022246eb2645f8cda383e90
C2=claim-9886a4b34787ad9b2c0fc31a61f4b74b
B=/Users/lei/.local/bin/lumen-science
"$B" project evidence compare --project "$ID" --claim-a "$C1" --claim-b "$C2" --store "$STORE" > "$SCRATCH/evidence-compare.json"
"$B" project multimodal --project "$ID" --store "$STORE" > "$SCRATCH/multimodal.json"
"$B" project review --project "$ID" --reviewer r1 --verdict supported --store "$STORE" > "$SCRATCH/review.json"
"$B" project collaborator --project "$ID" --owner u1 --invitee u2 --store "$STORE" > "$SCRATCH/collaborator.json"
"$B" project migrate --run v1-run --owner u1 --title migrated --question Q --store "$STORE" > "$SCRATCH/migrate.json"
for f in evidence-compare.json multimodal.json review.json collaborator.json migrate.json; do
  echo "--- $f ---"
  python3 -c "import json;d=json.load(open('$SCRATCH/$f'));print(list(d.keys())[:8])"
done
