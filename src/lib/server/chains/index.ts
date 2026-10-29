import type { ChainId } from '../../types';
import { hemkop, willys } from './axfood';
import { citygross } from './citygross';
import { lidl } from './lidl';
import type { ChainAdapter } from './types';

// To add a chain: implement ChainAdapter in its own file and register it here
// (and add it to CHAINS in src/lib/types.ts with online: true).
// ICA and Coop have no adapter: they have no public price data (see README).
export const adapters: Partial<Record<ChainId, ChainAdapter>> = { willys, hemkop, lidl, citygross };
