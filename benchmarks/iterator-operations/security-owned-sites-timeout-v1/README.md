# Historical Miri job deadline

Run41198356374 shard1 exhausted its60-minute job budget after8 of10 exact tests completed. The pipeline promise test was interrupted and the final regexp test never started. The original log and job conclusions are retained. Current CI keeps16shards, every exact test and both strict Miri flags; only the job budget grows to90minutes. Native180second/512MiB and Test262 limits remain unchanged. The old run is not a passing safety result.
