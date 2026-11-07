#!/bin/zsh
# usage: run.sh name page mode test secs [csp]
cd "$(dirname $0)/.."
name=$1 page=$2 mode=$3 test=$4 secs=$5 csp=$6 book=${7:-hostile.epub}
log=results/$name.log hits=results/$name.hits
rm -f $log $hits; touch $hits
python3 tools/hits.py $PWD/$hits & srv=$!
sleep 0.4
env SPIKE_OUT=$PWD/$log SPIKE_BOOKS=${BOOKS:-$PWD/books} SPIKE_SHOTS=$PWD/results/shots SPIKE_SECS=$secs ${GUARD:+SPIKE_GUARD=1} ${RULES:+SPIKE_RULES=1} SPIKE_PAGES="main=main.html,reader-1=$page" \
  SPIKE_CFG="{\"id\":\"$book\",\"mode\":\"$mode\",\"test\":\"$test\",\"secs\":${SECS:-[]}}" ${csp:+SPIKE_HDR="$csp"} ./target/release/spike
kill $srv
