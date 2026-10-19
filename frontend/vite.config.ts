import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const config = defineConfig({
	server: {
		host: true,
		port: 3000,
		strictPort: true,
		hmr: {
			clientPort: 3000,
		},
		proxy: {
			"/courseviewer.admin.v1.AdminService": "http://127.0.0.1:8080",
			"/api": "http://127.0.0.1:8080",
			"/local-media": "http://127.0.0.1:8080",
		},
	},
	resolve: { tsconfigPaths: true },
	plugins: [
		tailwindcss(),
		tanstackRouter({ target: "react", autoCodeSplitting: true }),
		viteReact(),
	],
});

export default config;
