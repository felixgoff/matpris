// Number formatting for the English UI: "19.40 kr", "12.93 kr/l", "1.1 km".
const money = new Intl.NumberFormat('en-GB', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const short = new Intl.NumberFormat('en-GB', { maximumFractionDigits: 1 });

export const kr = (n: number) => `${money.format(n)} kr`;
export const km = (n: number) => `${short.format(n)} km`;

/** "15.90" split into kronor and öre, for prices set as a shelf label ("15⁹⁰"). */
export const krParts = (n: number) => money.format(n).split('.') as [string, string];
