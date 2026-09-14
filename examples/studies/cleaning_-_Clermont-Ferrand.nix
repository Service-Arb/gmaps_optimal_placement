let clermont = import ../_Clermont-Ferrand.nix; in
{
  name = "cleaning_-_Clermont-Ferrand";

  inherit (clermont) area grid;

  poi = {
    source = "google_places";
    queries = [
      "entreprise de nettoyage"
      "société de nettoyage"
      "femme de ménage"
      "ménage à domicile"
      "aide à domicile"
      "nettoyage de bureaux"
      "nettoyage fin de chantier"
      "nettoyage vitres"
    ];
    tiles = 3;

    # A cleaning firm is direct competition; a services-à-la-personne agency selling childcare and
    # gardening alongside the ménage hour is not who the search lands on.
    # First match wins, so this stays a list.
    tier = [
      {
        name = "cleaning";
        weight = 1.0;
        # No "entretien": in this inventory it names boiler servicing and nothing that also cleans.
        match = "nettoyage|ménage|menage|repassage|propreté|proprete|clean|vitrerie|hygiène|hygiene";
      }
      {
        name = "home_help";
        weight = 0.35;
        match = "domicile|à la personne|a la personne|\\badmr\\b|\\bo2\\b|shiva|azaé|azae|maison (et|&) services|centre services|merci\\+|multiservice|multi-service|multi service|conciergerie";
      }
    ];

    # "nettoyage" is the same word for a car, a shirt and a flat: the queries pull in car washes,
    # pressings and the shops that sell the products. Chimney sweeps and pest control answer a
    # different call entirely, and "aide à domicile" is half nursing.
    drop = {
      match = "pressing|laverie|blanchisserie|teinturerie|5 à sec|auto\\b|automobile|voiture|carrosserie|lavage|piscine|ramonage|nuisible|dératisation|deratisation|désinsectisation|desinsectisation|espaces verts|paysagiste|élagage|elagage|droguerie|magasin|grossiste|fourniture|infirmi";
      types = [ "car_wash" "hardware_store" "home_goods_store" "store" "health" ];
    };
  };

  # Square metres, not households: what is bought is hours, and hours follow floor area. That the
  # house outweighs the flat then needs no coefficient — INSEE already measured it.
  column = clermont.column ++ [
    { name = "senior_share"; expr = "(ind_65_79 + ind_80p) / max(ind, 1)"; }
  ];

  # Nobody has to hire a cleaner, so income bites nearly as hard as it does on detailing — the 50 %
  # crédit d'impôt is what keeps the exponent under it. The senior term is the other half of the
  # market: aide ménagère, part-funded by the APA, and not chosen on price.
  model = {
    demand = "men_surf * (0.7 + senior_share) * (max(nv, 4000) / 22000) ^ 1.5";
    # The cleaner drives to the customer and does it again every week, so the unpaid commute is
    # priced five times over — a tighter catchment than the plumber's one-off callout.
    lambda_m = 3000;
  };

  # Not `poi.queries`: those name the trade the way the trade names itself, which is exactly the
  # variation the name coefficient cannot be fitted on. No company is called "femme de ménage" and
  # that is what the household types.
  rank = {
    nodes = 32;
    radius_m = 4000;
    term = [
      { text = "femme de ménage"; weight = 1.0; }
      { text = "nettoyage bureaux"; weight = 0.6; }
      { text = "nettoyage fin de chantier"; weight = 0.3; }
    ];
  };

  layer = clermont.layer ++ [
    {
      name = "Dwelling floor area (m²)";
      expr = "men_surf";
      note = "What there is to clean.";
    }
    {
      name = "Average dwelling (m²)";
      expr = "men_surf / max(men, 1)";
      scale = "linear";
      note = "A per-household rate, not a density — the big-house suburbs light up on a thin population.";
    }
    {
      name = "Seniors 65+";
      expr = "ind_65_79 + ind_80p";
      note = "The half of the market that buys aide ménagère rather than a cleaner, and buys it every week.";
    }
  ];

  # "femme de ménage" is searched by the household hiring one and by the woman looking for the job in
  # roughly the same breath, so this group lives or dies on `drop`.
  searches = clermont.searches // {
    group = [
      {
        name = "ménage";
        seed = [ "femme de ménage" "ménage à domicile" ];
        match = "ménage|menage|repassage|aide ménagère|aide menagere|domicile";
        drop = "emploi|salaire|recrutement|\\bjobs?\\b|\\bcdi\\b|\\bcdd\\b|formation|stage|indeed|pôle emploi|pole emploi|france travail|smic|convention collective|fiche de paie|devenir";
      }
      {
        name = "nettoyage pro";
        seed = [ "entreprise de nettoyage" "nettoyage de bureaux" ];
        match = "entreprise|société|societe|bureau|professionnel|industriel|copropriété|copropriete|immeuble|parties communes|chantier|locaux|local";
        drop = "emploi|salaire|recrutement|\\bjobs?\\b|formation|stage|franchise|création|creation";
      }
      {
        name = "spécialisé";
        seed = [ "nettoyage vitres" "nettoyage canapé" ];
        match = "vitre|vitrerie|canapé|canape|moquette|tapis|matelas|fauteuil|toiture|façade|facade|démoussage|demoussage|karcher|kärcher";
      }
    ];
  };
}
