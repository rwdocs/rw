export class Ui {
  mobileMenuOpen = $state(false);
  tocPopoverOpen = $state(false);

  openMobileMenu = () => {
    this.mobileMenuOpen = true;
  };

  closeMobileMenu = () => {
    this.mobileMenuOpen = false;
  };

  toggleTocPopover = () => {
    this.tocPopoverOpen = !this.tocPopoverOpen;
  };

  closeTocPopover = () => {
    this.tocPopoverOpen = false;
  };
}
