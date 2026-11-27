#!/bin/bash
# drive.sh <checkout> <base|head> <out dir>: on the hub's Connectors page, host
# TEST-vbox (behind proxy.py), Add connector > Linear > Connect with mcp.add
# armed, then Check again, Remove Linear > Confirm with mcp.remove armed. One
# drive per viewport; the proxy is armed again before each.
set -u
CO=$1; REF=$2; OUT=$3
cd "$CO" || exit 1
PICK='{"expect":"role:heading:Connectors"},{"click":"role:button:Connector host"},{"click":"role:menuitemradio:TEST-vbox"},{"click":{"role":"button","name":"Claude","exact":true}},{"expect":{"css":".connectors-card:has-text(\"Claude\") .connector","hasText":"chart"}}'
ADD='{"click":{"css":".connectors-card:has-text(\"Claude\") button","hasText":"Add connector"}},{"click":{"role":"button","name":"Linear","exact":true}},{"click":{"role":"button","name":"Connect","exact":true,"within":"role:dialog:Add connector"}}'
BACK='{"expect":{"css":".connectors-card:has-text(\"Claude\") button","hasText":"Add connector"}}'
REMOVE='{"click":"role:button:Check again"},{"expect":{"css":".connectors-card:has-text(\"Claude\") .connector","hasText":"Linear"}},{"click":"role:button:Remove Linear"},{"click":"role:button:Confirm removing Linear"}'
if [ "$REF" = base ]; then
  # The page gets the error (no hostId) and shows it nowhere; the row waits forever.
  STEPS="[$PICK,$ADD,{\"expect\":\"text:Host offline\"},$BACK,{\"wait\":3000},{\"expect\":\"role:dialog:Add connector\",\"count\":0},{\"expect\":\"css:.banner-error\",\"count\":0},{\"expect\":\"text:Lost the connection\",\"count\":0},{\"shot\":\"add-lost\"},$REMOVE,{\"expect\":\"text:Host offline\"},$BACK,{\"wait\":8000},{\"expect\":{\"css\":\".connector\",\"hasText\":\"Linear\"},\"text\":\"Removing…\"},{\"expect\":\"text:Lost the connection\",\"count\":0},{\"shot\":\"remove-lost\"}]"
else
  STEPS="[$PICK,$ADD,{\"expect\":\"css:.banner-error\",\"text\":\"Lost the connection to TEST-vbox before it answered. This may have gone through; check once it's back.\"},{\"expect\":\"text:Host offline\"},{\"shot\":\"add-lost-host-down\"},$BACK,{\"expect\":\"css:.banner-error\",\"text\":\"Lost the connection to TEST-vbox\"},{\"shot\":\"add-lost\"},{\"click\":\"role:button:Dismiss\"},{\"expect\":\"css:.banner-error\",\"count\":0},$REMOVE,{\"expect\":\"css:.banner-error\",\"text\":\"Lost the connection to TEST-vbox\"},$BACK,{\"expect\":{\"css\":\".connector\",\"hasText\":\"Linear\"},\"text\":\"Lost the connection to TEST-vbox before it answered. This may have gone through; check once it's back.\"},{\"expect\":\"role:button:Remove Linear\"},{\"expect\":\"text:Removing…\",\"count\":0},{\"shot\":\"remove-lost\"},{\"click\":\"role:button:Dismiss\"},{\"click\":\"role:button:Check again\"},{\"expect\":{\"css\":\".connectors-card:has-text(\\\"Claude\\\") .connector\",\"hasText\":\"Linear\"},\"count\":0},{\"shot\":\"checked-again\"}]"
fi
for V in desktop mobile; do
  echo "mcp.add,mcp.remove" > "$OUT/armed"
  verify/yv drive connectors --iso hub --viewport $V --allow-errors --out "$OUT/$V" --steps "$STEPS" > "$OUT/drive-$V.json" 2>&1
  echo "$REF $V drive exit $? armed-left='$(cat "$OUT/armed")'"
  jq -c '{ok, problems: [.problems[]? | .msg], failedApi: [.. | .failedApi? // empty] }' "$OUT/drive-$V.json" 2>/dev/null | cut -c1-1500
  # What the box holds now, and put it back as it was for the next run.
  verify/yv api '{"op":"mcp.remove","engine":"claude","account":"default","server":"linear"}' --iso box > /dev/null 2>&1
  sleep 2
done
cat "$OUT/proxy.log"
echo "$REF drives done"
