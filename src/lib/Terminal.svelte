<script lang="ts">
	import { Terminal } from '@xterm/xterm';
	// import { FitAddon } from '@xterm/addon-fit';
	import { onMount, onDestroy } from 'svelte';
	import '@xterm/xterm/css/xterm.css';

	let term: Terminal;
	let terminalEle: HTMLDivElement;
	let socket: WebSocket;

	onMount(() => {
		const term = new Terminal({
			cursorBlink: true
		});
		term.open(terminalEle);

		const socket = new WebSocket('ws://127.0.0.1:8080');

		socket.onopen = () => {
			term.write('Connected to the server.\r\n');
			term.focus();

			const initialSize = { cols: term.cols, rows: term.rows };
			const initialMsg = JSON.stringify({
				type: 'resize',
				cols: initialSize.cols,
				rows: initialSize.rows
			});
			socket.send(initialMsg);
		};

		socket.onmessage = (event) => {
			term.write(event.data);
		};

		socket.onclose = () => {
			term.write('\r\nConnection closed.\r\n');
		};

		// term.onResize({
		// 	cols: term.cols,
		// 	rows: term.rows
		// });

		term.onData((data) => {
			socket.send(data);
		});
	});

	onDestroy(() => {
		socket.close();
		term.dispose();
	});
</script>

<div bind:this={terminalEle} class="terminal"></div>

<style>
	.terminal {
		height: 100%;
		width: 100%;
	}
</style>
