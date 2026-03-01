export interface Config {
  /** @deepVisibility backend */
  rw?: {
    /** Local directory containing documentation source files. */
    projectDir?: string;
    /** URL prefix for generated links (e.g. "/rw-docs"). */
    linkPrefix?: string;
    /** S3 storage configuration for deployed environments. */
    s3?: {
      /** S3 bucket name. */
      bucket: string;
      /** Entity identifier (prefix) within the bucket. */
      entity: string;
      /** AWS region. */
      region?: string;
      /** Custom S3 endpoint URL. */
      endpoint?: string;
      /** Root path within the bucket. */
      bucketRootPath?: string;
    };
  };
}
