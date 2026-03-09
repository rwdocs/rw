export class Ui {
  mobileMenuOpen = $state(false);

  openMobileMenu = () => {
    this.mobileMenuOpen = true;
  };

  closeMobileMenu = () => {
    this.mobileMenuOpen = false;
  };
}
