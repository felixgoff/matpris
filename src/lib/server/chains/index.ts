import type { ChainId } from '../../types';
import { hemkop, willys } from './axfood';
import { citygross } from './citygross';
import { coop } from './coop';
import { ica } from './ica';
import { lidl } from './lidl';
import type { ChainAdapter } from './types';

// To add a chain: implement ChainAdapter in its own file and register it here
// (and set online: true in CHAINS, src/lib/types.ts).
// ICA searches the shop's own API server-side with a WAF session cookie file
// (scripts/ica-warm.mjs mints it; see ica.ts).
export const adapters: Partial<Record<ChainId, ChainAdapter>> = { willys, hemkop, lidl, citygross, coop, ica };
