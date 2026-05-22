export let a = false;
export let l = false;
export let t = false;

export function enable_a() {
	a = true;
}

export function disable_a() {
	a = false;
}

export function enable_l() {
	l = true;
}

export function enable_t() {
	t = true;
}

export let $document;

export function init() {
	if (!window) return;
	$document = document;
}