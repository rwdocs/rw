import { on } from "svelte/events";

/**
 * Make every `.tabs` element within `container` interactive: click to select,
 * ArrowLeft/ArrowRight (wrapping), Home and End to move between tabs.
 *
 * @returns A cleanup function that removes all listeners
 */
export function initializeTabs(container: HTMLElement): () => void {
  const tabContainers = container.querySelectorAll<HTMLElement>(".tabs");
  const cleanups: (() => void)[] = [];

  for (const tabContainer of tabContainers) {
    cleanups.push(initializeTabContainer(tabContainer));
  }

  return () => {
    for (const cleanup of cleanups) {
      cleanup();
    }
  };
}

function initializeTabContainer(container: HTMLElement): () => void {
  const tablist = container.querySelector<HTMLElement>('[role="tablist"]');
  if (!tablist) return () => {};

  const tabs = Array.from(tablist.querySelectorAll<HTMLElement>('[role="tab"]'));
  if (tabs.length === 0) return () => {};

  const offClicks = tabs.map((tab) => on(tab, "click", () => activateTab(tab, tabs)));
  const offKeydown = on(tablist, "keydown", (event) => handleTabKeydown(event, tabs));

  return () => {
    for (const off of offClicks) off();
    offKeydown();
  };
}

function activateTab(selectedTab: HTMLElement, allTabs: HTMLElement[]): void {
  for (const tab of allTabs) {
    const selected = tab === selectedTab;
    tab.setAttribute("aria-selected", String(selected));
    tab.setAttribute("tabindex", selected ? "0" : "-1");
    const panelId = tab.getAttribute("aria-controls");
    const panel = panelId ? document.getElementById(panelId) : null;
    if (panel) panel.hidden = !selected;
  }
  selectedTab.focus();
}

function handleTabKeydown(event: KeyboardEvent, tabs: HTMLElement[]): void {
  const currentIndex = tabs.indexOf(event.target as HTMLElement);
  if (currentIndex === -1) return;

  let newIndex: number;
  switch (event.key) {
    case "ArrowLeft":
      newIndex = currentIndex > 0 ? currentIndex - 1 : tabs.length - 1;
      break;
    case "ArrowRight":
      newIndex = currentIndex < tabs.length - 1 ? currentIndex + 1 : 0;
      break;
    case "Home":
      newIndex = 0;
      break;
    case "End":
      newIndex = tabs.length - 1;
      break;
    default:
      return;
  }

  event.preventDefault();
  activateTab(tabs[newIndex], tabs);
}
