/**
 * Initialize interactive tabs functionality within a container element.
 *
 * Adds click handlers and keyboard navigation (Arrow keys, Home, End)
 * to all `.tabs` elements within the container.
 *
 * @param container - The container element to search for tabs
 * @returns A cleanup function to remove all event listeners
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

/**
 * Initialize a single tab container.
 * @returns A cleanup function to remove all event listeners
 */
function initializeTabContainer(container: HTMLElement): () => void {
  const tablist = container.querySelector<HTMLElement>('[role="tablist"]');
  if (!tablist) return () => {};

  const tabs = Array.from(tablist.querySelectorAll<HTMLElement>('[role="tab"]'));
  if (tabs.length === 0) return () => {};

  const clickHandlers: Array<{ tab: HTMLElement; handler: () => void }> = [];

  // Add click handlers to all tabs
  for (const tab of tabs) {
    const handler = () => {
      activateTab(container, tab, tabs);
    };
    tab.addEventListener("click", handler);
    clickHandlers.push({ tab, handler });
  }

  // Add keyboard navigation to tablist
  const keydownHandler = (event: KeyboardEvent) => {
    handleTabKeydown(event, container, tabs);
  };
  tablist.addEventListener("keydown", keydownHandler);

  return () => {
    for (const { tab, handler } of clickHandlers) {
      tab.removeEventListener("click", handler);
    }
    tablist.removeEventListener("keydown", keydownHandler);
  };
}

/**
 * Activate a specific tab and show its panel.
 */
function activateTab(
  container: HTMLElement,
  selectedTab: HTMLElement,
  allTabs: HTMLElement[],
): void {
  // Deactivate all tabs
  for (const tab of allTabs) {
    tab.setAttribute("aria-selected", "false");
    tab.setAttribute("tabindex", "-1");

    // Hide associated panel
    const panelId = tab.getAttribute("aria-controls");
    if (panelId) {
      const panel = container.querySelector<HTMLElement>(`#${panelId}`);
      if (panel) {
        panel.hidden = true;
      }
    }
  }

  // Activate selected tab
  selectedTab.setAttribute("aria-selected", "true");
  selectedTab.setAttribute("tabindex", "0");
  selectedTab.focus();

  // Show associated panel
  const selectedPanelId = selectedTab.getAttribute("aria-controls");
  if (selectedPanelId) {
    const selectedPanel = container.querySelector<HTMLElement>(`#${selectedPanelId}`);
    if (selectedPanel) {
      selectedPanel.hidden = false;
    }
  }
}

/**
 * Handle keyboard navigation within a tablist.
 *
 * - ArrowLeft/ArrowRight: Move to adjacent tab
 * - Home: Move to first tab
 * - End: Move to last tab
 */
function handleTabKeydown(event: KeyboardEvent, container: HTMLElement, tabs: HTMLElement[]): void {
  const currentTab = event.target as HTMLElement;
  const currentIndex = tabs.indexOf(currentTab);

  if (currentIndex === -1) return;

  let newIndex: number;

  switch (event.key) {
    case "ArrowLeft":
      // Move to previous tab, wrap to end if at start
      newIndex = currentIndex > 0 ? currentIndex - 1 : tabs.length - 1;
      break;
    case "ArrowRight":
      // Move to next tab, wrap to start if at end
      newIndex = currentIndex < tabs.length - 1 ? currentIndex + 1 : 0;
      break;
    case "Home":
      newIndex = 0;
      break;
    case "End":
      newIndex = tabs.length - 1;
      break;
    default:
      return; // Don't prevent default for other keys
  }

  event.preventDefault();
  activateTab(container, tabs[newIndex], tabs);
}
