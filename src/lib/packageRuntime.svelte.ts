let packageRevision = $state(0);

export const packageRuntime = {
  get revision() {
    return packageRevision;
  },
  markPackagesChanged() {
    packageRevision += 1;
  }
};
