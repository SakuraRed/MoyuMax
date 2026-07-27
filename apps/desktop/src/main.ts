import "./styles.css";
import "./moyu.css";

import { mount } from "svelte";

import App from "./App.svelte";

const target = document.querySelector<HTMLElement>("#app");
if (!target) {
  throw new Error("MoyuMax root element is missing");
}

mount(App, { target });
