import { describe, expect, it } from 'vitest';
import type { ChainId, Store } from '../../types';
import { pickArea } from './radius';

const chains: ChainId[] = ['willys', 'hemkop', 'lidl'];
// ~0.009° latitude ≈ 1 km
const ring = (n: number, km: number, start = 0): Store[] =>
	Array.from({ length: n }, (_, i) => ({
		id: `${start + i}`,
		chain: chains[(start + i) % 3],
		name: `S${start + i}`,
		lat: 59 + (km / 111) * Math.cos(i),
		lng: 18 + ((km / 111) * Math.sin(i)) / Math.cos((59 * Math.PI) / 180),
		address: ''
	}));
const center = { lat: 59, lng: 18 };

describe('pickArea', () => {
	it('keeps the circle small in a dense city', () => {
		const { radiusKm, stores } = pickArea(center, [...ring(15, 1), ...ring(30, 10, 15)]);
		expect(radiusKm).toBe(1.5);
		expect(stores.length).toBe(15);
	});

	it('grows the circle in the countryside', () => {
		const { radiusKm, stores } = pickArea(center, ring(12, 14));
		expect(radiusKm).toBeGreaterThan(14);
		expect(radiusKm).toBeLessThanOrEqual(30);
		expect(stores.length).toBe(12);
	});

	it('stops at the cap, or stretches to the nearest store when nothing is inside', () => {
		expect(pickArea(center, ring(3, 20)).radiusKm).toBeLessThanOrEqual(30);
		const remote = pickArea(center, ring(2, 80));
		expect(remote.radiusKm).toBeCloseTo(80, 0);
		expect(remote.stores.length).toBeGreaterThanOrEqual(1);
	});

	it('caps stores per chain', () => {
		const { stores } = pickArea(center, ring(60, 1));
		const willys = stores.filter((s) => s.chain === 'willys');
		expect(willys.length).toBe(5);
		expect(stores.length).toBe(15);
	});
});
