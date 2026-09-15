/* What every French study shares regardless of city or trade: the grid, how INSEE spells the
   numbers it publishes, and who prices a keyword in French. A city file in [locations/] takes this
   and adds its frame; a trade in [trades/] is a function of that. */
{
  grid = {
    source = "insee_filosofi_200m";
    vintage = 2021;
  };

  # INSEE publishes the standard of living summed over individuals, not per head.
  column = [
    { name = "nv"; expr = "ind_snv / max(ind, 1)"; }
  ];

  #Q: potentially harden this, so as to move out of the config, - I don't think this'll be changing
  layer = [
    {
      name = "Population";
      expr = "ind";
      note = "Raw head count per 200 m cell.";
    }
    {
      name = "Households";
      expr = "men";
      note = "Fiscal households per cell.";
    }
    {
      name = "Standard of living (€/yr)";
      expr = "nv";
      scale = "linear";
      note = "Mean disposable income per consumption unit. A per-person rate, not a density — thinly populated affluent suburbs light up.";
    }
    {
      name = "Households in houses";
      expr = "men_mais";
      note = "House rather than flat: a private driveway, a private boiler, no syndic standing between the household and the trade.";
    }
    {
      name = "Households in flats";
      expr = "men_coll";
      note = "The other half of the split. What the flat shares — the parties communes, the riser, the roof — is let by a syndic, not by the household on the map.";
    }
  ];

  searches = {
    provider = "google_ads";
    language = "fr";
  };
}
