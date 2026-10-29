import type { ChainId } from '../../types';
import { hemkop, willys } from './axfood';
import { citygross } from './citygross';
import { coop } from './coop';
import { lidl } from './lidl';
import type { ChainAdapter } from './types';

// To add a chain: implement ChainAdapter in its own file and register it here
// (and set online: true in CHAINS, src/lib/types.ts).
// ICA has no adapter: its sites are behind a CAPTCHA-bearing AWS WAF (see README);
// a crawlable catalogue lives in data/ica-*.json but is incomplete without an adapter.
export const adapters: Partial<Record<ChainId, ChainAdapter>> = { willys, hemkop, lidl, citygross, coop };
