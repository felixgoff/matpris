import type { ChainId } from '../../types';
import { hemkop, willys } from './axfood';
import { citygross } from './citygross';
import { coop } from './coop';
import { ica } from './ica';
import { lidl } from './lidl';
import type { ChainAdapter } from './types';

// To add a chain: implement ChainAdapter in its own file and register it here
// (and set online: true in CHAINS, src/lib/types.ts).
// ICA searches the locally assembled price index data/ica-catalogue.json (see README)
// but is offline (CHAINS.ica.online) until that index has real crawled content.
export const adapters: Partial<Record<ChainId, ChainAdapter>> = { willys, hemkop, lidl, citygross, coop, ica };
