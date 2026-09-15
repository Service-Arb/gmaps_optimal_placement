Where fitted ranking models come from, and how they are made to compete.

`gmaps_optimal_placement_core::rank` is the fitted model everyone reads: wasm-safe, no optimiser.
This crate is the other half — the optimiser, the entrants, and the held-out loss that tells them
apart. The split is what keeps an entrant's dependencies off the crate the browser links.

- [`Observed`] — the orderings on disk, turned into choice sets. A search returned some businesses
  and not others, and the ones it did not return are what identify anything at all; who *could* have
  been returned is decided here, once, for every entrant.
- [`Ordering`] — one of those choice sets, handed out as raw businesses. A strategy that cannot
  choose its own features is a coefficient set wearing a trait.
- [`Strength`] / [`Strategy`] — the entrant interface. `fit` returns a fresh [`Strength`] rather than
  mutating the entrant, so fold independence is structural.
- [`Linear`] — the ladder that ships: `FLAT`, `REVIEWS`, `FITTED`, and one leave-one-feature-out per
  feature. All one impl over Adam's `on` mask.
- [`League`] — k-fold cross-validation, split by [`Region`](gmaps_optimal_placement_sources::Region)
  so no node's geometry crosses a fold boundary, and the table it prints.
- [`fit`] — the Adam loop behind `gmaps_optimal_placement fit`, which produces the constant
  `core::rank::COEF` is pasted from. [`League`] produces the argument for which constant.
