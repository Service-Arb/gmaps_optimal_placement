/* What every Clermont-Ferrand study shares regardless of trade: the frame, the grid under it, the
   raw INSEE readouts, and the location Google resolves search volume against. */
{
  area = {
    # Clermont-Ferrand agglomeration, generous: Riom (N) to Issoire-ward (S), Volvic (W) to Lezoux-ward (E).
    bbox = { lat = [ 45.55 45.95 ]; lon = [ 2.90 3.40 ]; };
    center = [ 45.7797 3.0863 ];
    zoom = 12;
  };

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
  ];

  searches = {
    provider = "google_ads";
    place = "Clermont-Ferrand,Auvergne-Rhone-Alpes,France";
    language = "fr";
  };
}
