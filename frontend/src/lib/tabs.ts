/**
 * Initialize interactive tabs functionality within a container element.
 *
 * Adds click handlers and keyboard navigation (Arrow keys, Home, End)
 * to all `.tabs` elements within the container.
 *
 * @param container - The container element to search for tabs
 */
export function initializeTabs(container: HTMLElement): void {
  const tabContainers = container.querySelectorAll<HTMLElement>(".tabs");

  for (const tabContainer of tabContainers) {
    initializeTabContainer(tabContainer);
  }
}

/**
 * Initialize a single tab container.
 */
function initializeTabContainer(container: HTMLElement): void {
  const tablist = container.querySelector<HTMLElement>('[role="tablist"]');
  if (!tablist) return;

  const tabs = Array.from(tablist.querySelectorAll<HTMLElement>('[role="tab"]'));
  if (tabs.length === 0) return;

  // Add click handlers to all tabs
  for (const tab of tabs) {
    tab.addEventListener("click", () => {
      activateTab(container, tab, tabs);
    });
  }

  // Add keyboard navigation to tablist
  tablist.addEventListener("keydown", (event) => {
    handleTabKeydown(event, container, tabs);
  });
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

  let newIndex: number | null = null;

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

  if (newIndex !== null) {
    event.preventDefault();
    activateTab(container, tabs[newIndex], tabs);
  }
}
