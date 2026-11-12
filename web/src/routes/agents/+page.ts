// The directory is a database view: search, paging and "last active" only mean
// anything at request time, so this route is served by the client-side shell.
export const ssr = false;
