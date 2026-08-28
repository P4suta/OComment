import { build, context } from "esbuild";

const production = process.argv.includes("--production");
const watch = process.argv.includes("--watch");

/** @type {import("esbuild").BuildOptions} */
const options = {
	entryPoints: ["src/extension.ts"],
	outfile: "dist/extension.js",
	bundle: true,
	// NOTE: `vscode` is supplied by the extension host at run time and has no
	// NOTE: package on disk, so it is the one import that must stay external.
	external: ["vscode"],
	format: "cjs",
	platform: "node",
	// NOTE: engines.vscode ^1.96.0 is Electron 32, which is Node 20.
	target: "node20",
	sourcemap: !production,
	minify: production,
	logLevel: "info",
};

if (watch) {
	const builder = await context(options);
	await builder.watch();
} else {
	await build(options);
}
