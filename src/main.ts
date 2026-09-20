if (new URLSearchParams(location.search).get('view') === 'status') {
  void import('./compact');
} else {
  void import('./dashboard');
}

export {};
