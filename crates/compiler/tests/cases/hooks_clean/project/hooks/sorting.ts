export const isSorted = ({ response }: { response: { body: number[] } }) =>
  response.body.every((item, at, all) => at === 0 || all[at - 1] <= item);
