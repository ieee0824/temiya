// Independent integer oracles from engine_benchmark_results.rs at this pin.
export const revision = '007556adccb8fb4329236932ca8175f8e8d119d2';
export const fixtureSha256 = '177eb80ae73d1081eb620f747b4703e2a9f2731e0255bb60131b73b7dcac6464';
export const oracle = [
  ['arith', 2000000, 64], ['prop-mono', 1000000, 500003500000],
  ['prop-mega', 500000, 1750000], ['call', 1000000, 1000000],
  ['closure-alloc', 300000, 715003], ['object-alloc', 300000, 730003],
  ['string-concat', 200000, 2494], ['array', 500000, 124999750000],
  ['primitive-string-property', 500000, 499997],
  ['primitive-string-method', 500000, 999856], ['proto-method', 500000, 750000],
];
