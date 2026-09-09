// The only file that names `google.maps`. It owns the map instance, the markers and the canvas
// overlay; every number it paints comes from Rust. One instance per host element, reused across
// mounts.
//
// The canvas loop lives here for the same reason the projection does: it needs `fromLatLngToDivPixel`
// every frame. The contract with Rust is four arrays and an opacity — see `cells`.
//
// Nothing throws across the boundary: under `panic=abort` a rejected promise reaching wasm kills the
// app, so every entry point returns a banner string instead.

const S = new WeakMap();

/// `shell()` resolves this once the Maps bootstrap has run.
const ready = () => window.__mapsReady ?? Promise.reject(new Error('the Maps bootstrap never ran'));

export async function mount(el, lat, lng, zoom, onClick, onMove, onOut, onPin) {
	try {
		if (S.has(el)) return null;
		await ready();
		const map = new google.maps.Map(el, {
			center: { lat, lng }, zoom, mapTypeId: 'roadmap',
			streetViewControl: false, fullscreenControl: false, clickableIcons: false,
			styles: [{ featureType: 'poi', elementType: 'labels', stylers: [{ visibility: 'off' }] }],
		});
		const cv = document.createElement('canvas');
		const s = {
			map, cv, ctx: cv.getContext('2d'), onPin,
			ringX: null, ringY: null, colors: null, shown: null, opacity: 0.62,
			markers: [], tiers: null, pins: new Map(), iw: new google.maps.InfoWindow(),
		};
		S.set(el, s);

		s.overlay = new google.maps.OverlayView();
		s.overlay.onAdd = function () { this.getPanes().overlayLayer.appendChild(cv); };
		s.overlay.draw = () => paint(el, s);
		s.overlay.setMap(map);

		map.addListener('click', e => onClick(e.latLng.lat(), e.latLng.lng()));
		map.addListener('mousemove', e => {
			const r = el.getBoundingClientRect();
			onMove(e.latLng.lat(), e.latLng.lng(), e.domEvent.clientX - r.left, e.domEvent.clientY - r.top);
		});
		map.addListener('mouseout', () => onOut());
		return null;
	} catch (e) {
		return `⚠ map failed to load — ${(e && e.message) || e}`;
	}
}

/// `ringX`/`ringY` are 4 Web-Mercator corners per cell, `colors` 3 bytes per cell, `shown` one flag.
export function cells(el, ringX, ringY, colors, shown) {
	const s = S.get(el);
	if (!s) return;
	Object.assign(s, { ringX, ringY, colors, shown });
	paint(el, s);
}

export function opacity(el, v) {
	const s = S.get(el);
	if (!s) return;
	s.opacity = v;
	paint(el, s);
}

function paint(el, s) {
	const proj = s.overlay.getProjection(), b = s.map.getBounds();
	if (!proj || !b || !s.shown) return;
	// derive scale from two known world points: robust to fractional zoom
	const p0 = proj.fromLatLngToDivPixel(new google.maps.LatLng(0, 0));
	const p1 = proj.fromLatLngToDivPixel(new google.maps.LatLng(0, 90));
	const k = (p1.x - p0.x) / 64, ox = p0.x - 128 * k, oy = p0.y - 128 * k;
	const nw = proj.fromLatLngToDivPixel(new google.maps.LatLng(b.getNorthEast().lat(), b.getSouthWest().lng()));
	const W = el.offsetWidth, H = el.offsetHeight, dpr = window.devicePixelRatio || 1;
	const { cv, ctx } = s;
	if (cv.width !== W * dpr || cv.height !== H * dpr) { cv.width = W * dpr; cv.height = H * dpr; }
	cv.style.cssText = `position:absolute;pointer-events:none;left:${nw.x}px;top:${nw.y}px;width:${W}px;height:${H}px`;
	ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
	ctx.clearRect(0, 0, W, H);
	ctx.globalAlpha = s.opacity;
	for (let i = 0; i < s.shown.length; i++) {
		if (!s.shown[i]) continue;
		const x0 = ox + s.ringX[i * 4] * k - nw.x, y0 = oy + s.ringY[i * 4] * k - nw.y;
		if (x0 < -40 || y0 < -40 || x0 > W + 40 || y0 > H + 40) continue;
		ctx.fillStyle = `rgb(${s.colors[i * 3]},${s.colors[i * 3 + 1]},${s.colors[i * 3 + 2]})`;
		ctx.beginPath(); ctx.moveTo(x0, y0);
		for (let c = 1; c < 4; c++) ctx.lineTo(ox + s.ringX[i * 4 + c] * k - nw.x, oy + s.ringY[i * 4 + c] * k - nw.y);
		ctx.closePath(); ctx.fill();
	}
	ctx.globalAlpha = 1;
}

/// The whole competitor inventory, replacing whatever is there. Each entry carries its tier index
/// and its colour; the info window is presentation and stays here.
export function competitors(el, json) {
	const s = S.get(el);
	if (!s) return;
	for (const m of s.markers) m.setMap(null);
	s.markers = JSON.parse(json).map(c => {
		const scale = c.big ? 7 : 4.5;
		const m = new google.maps.Marker({
			position: { lat: c.lat, lng: c.lng }, title: c.name, zIndex: c.big ? 3 : 2,
			label: c.n_rev ? { text: String(c.n_rev), className: 'nrev', color: '#fff', fontSize: '10px', fontWeight: '700' } : null,
			icon: {
				path: google.maps.SymbolPath.CIRCLE, fillColor: c.color, fillOpacity: 0.95,
				strokeColor: '#fff', strokeWeight: 1.2, scale,
				// labelOrigin is in path units, so divide out the scale to keep the gap constant in pixels
				labelOrigin: new google.maps.Point(0, -(scale + 8) / scale),
			},
		});
		m.ti = c.ti;
		m.addListener('click', () => {
			s.iw.setContent(`<div style="font:13px system-ui;max-width:250px;color:#111">
				<b>${esc(c.name)}</b><br>${esc(c.addr)}<br>
				<span style="color:#555">${esc(c.kind)} · ${esc(c.tier)}</span><br>
				★ ${c.rating ?? '–'} (${c.n_rev} reviews)${c.tel ? '<br>' + esc(c.tel) : ''}
				${c.web ? `<br><a href="${esc(c.web)}" target="_blank" rel="noreferrer">website</a>` : ''}</div>`);
			s.iw.open(s.map, m);
		});
		return m;
	});
	showTiers(el, s.tiers ?? new Uint8Array(0));
}

/// One flag per tier. An empty array shows everything — the state before the panel has been wired.
export function showTiers(el, flags) {
	const s = S.get(el);
	if (!s) return;
	s.tiers = flags;
	for (const m of s.markers) m.setMap(flags.length === 0 || flags[m.ti] ? s.map : null);
}

/// The full pin set, replacing whatever is there. Every pin is clickable, whether or not it is
/// lettered — an unlabelled pin you cannot select is a pin you cannot delete.
export function pins(el, json) {
	const s = S.get(el);
	if (!s) return;
	for (const m of s.pins.values()) m.setMap(null);
	s.pins.clear();
	for (const p of JSON.parse(json)) {
		const m = new google.maps.Marker({
			position: { lat: p.lat, lng: p.lng }, map: s.map, zIndex: 9, title: p.title ?? '',
			label: p.label ? { text: p.label, color: '#000', fontSize: '11px', fontWeight: '700' } : null,
			icon: {
				path: google.maps.SymbolPath.CIRCLE, fillColor: p.color, fillOpacity: 1,
				strokeColor: '#000', strokeWeight: 1.5, scale: 11,
			},
		});
		m.addListener('click', () => s.onPin(p.id));
		s.pins.set(p.id, m);
	}
}

const esc = t => String(t ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
