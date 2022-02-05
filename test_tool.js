export const plugin_metadata = {
	name: 'Test Tool',
	author: 'firstbober',
	version: '0.1.0'
}

export function initialize() {
	log("Hello from my plugin!");
	log(this.create_tool());
}