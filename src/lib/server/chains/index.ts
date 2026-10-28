import type { ChainId } from '../../types';
import { hemkop, willys } from './axfood';
import { lidl } from './lidl';
import type { ChainAdapter } from './types';

// To add a chain: implement ChainAdapter in its own file and register it here
// (and add its name/colour to CHAINS in src/lib/types.ts).
export const adapters: Record<ChainId, ChainAdapter> = { willys, hemkop, lidl };
