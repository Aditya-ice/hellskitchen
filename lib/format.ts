/**
 * Formatting for the edge of the app.
 *
 * Money crosses the wire as an integer count of minor units and is only ever
 * turned into text here. Before this existed the UI wrote `${item.price}`
 * directly — unrounded, so a dish at 24.5 rendered as "$24.5" — with the
 * currency symbol hardcoded in four places and no locale anywhere.
 */

/**
 * How many minor units make one major unit for a currency.
 *
 * Asked of `Intl` rather than assumed to be 100: yen and won have no minor
 * unit at all, and dividing those by 100 would show a bill a hundred times too
 * small. Falls back to 2 for a currency the runtime does not know.
 */
function minorUnitDigits(currency: string, locale?: string): number {
  try {
    return (
      new Intl.NumberFormat(locale, { style: "currency", currency })
        .resolvedOptions().maximumFractionDigits ?? 2
    );
  } catch {
    return 2;
  }
}

/** Renders an integer count of minor units as money. */
export function formatMoney(
  minorUnits: number,
  currency: string,
  locale?: string,
): string {
  const digits = minorUnitDigits(currency, locale);
  const major = minorUnits / 10 ** digits;
  try {
    return new Intl.NumberFormat(locale, {
      style: "currency",
      currency,
    }).format(major);
  } catch {
    // An unknown currency code should still produce a readable number rather
    // than throwing inside a render.
    return `${major.toFixed(digits)} ${currency}`;
  }
}

/**
 * A duration in whole minutes, as the pass would say it.
 *
 * Past an hour "94m" stops being readable at a glance, which is the only thing
 * a ticket rail is for.
 */
export function formatMinutes(minutes: number): string {
  const whole = Math.max(0, Math.round(minutes));
  if (whole < 60) return `${whole}m`;
  const hours = Math.floor(whole / 60);
  const rest = whole % 60;
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

/** Pluralises a count without saying "1 items". */
export function plural(count: number, singular: string, plural?: string): string {
  return `${count} ${count === 1 ? singular : (plural ?? `${singular}s`)}`;
}
