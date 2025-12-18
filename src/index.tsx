/* @refresh reload */
import { render } from "solid-js/web";
import { LauncherProvider } from "./store";
import App from "./App";

render(
  () => (
    <LauncherProvider>
      <App />
    </LauncherProvider>
  ),
  document.getElementById("root") as HTMLElement
);
