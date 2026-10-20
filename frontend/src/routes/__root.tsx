import "@mantine/core/styles.css";
import "katex/dist/katex.min.css";
import "../styles.css";
import {
	createTheme,
	type MantineColorsTuple,
	MantineProvider,
} from "@mantine/core";
import { createRootRoute, Outlet } from "@tanstack/react-router";
import { AdminProvider } from "../admin/context";

const myColor: MantineColorsTuple = [
	"#fff0e4",
	"#ffe0cf",
	"#fac0a1",
	"#f69e6e",
	"#f28043",
	"#f06e27",
	"#f06418",
	"#d6530c",
	"#bf4906",
	"#a73c00",
];
const theme = createTheme({
	colors: { myColor },
	primaryColor: "myColor",
	primaryShade: 5,
	fontFamily:
		'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
	headings: { fontFamily: "inherit", fontWeight: "650" },
	defaultRadius: "sm",
	components: {
		Button: { defaultProps: { fw: 500 } },
		TextInput: { defaultProps: { size: "sm" } },
		Textarea: { defaultProps: { size: "sm" } },
		Select: { defaultProps: { size: "sm" } },
	},
});

export const Route = createRootRoute({ component: RootComponent });
function RootComponent() {
	return (
		<MantineProvider forceColorScheme="dark" theme={theme}>
			<AdminProvider>
				<Outlet />
			</AdminProvider>
		</MantineProvider>
	);
}
