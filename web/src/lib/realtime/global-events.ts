import { toast } from 'svelte-sonner';
import { socket } from './socket.svelte';

/** Panel-wide notifications, shown wherever the user happens to be. */
export function registerGlobalEvents() {
	const stops = [
		socket.on<{ level: string; message: string }>('panel', 'notification', (event) => {
			if (!event?.message) return;
			if (event.level === 'error') toast.error(event.message);
			else if (event.level === 'warning') toast.warning(event.message);
			else toast(event.message);
		})
	];
	return () => stops.forEach((stop) => stop());
}
