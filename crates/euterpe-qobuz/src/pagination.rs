use crate::error::QobuzError;

#[derive(Debug, Clone, Copy)]
pub struct PageRequest {
    pub limit: u32,
    pub offset: u32,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            limit: 500,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

pub async fn fetch_all_pages<T, F, Fut>(
    mut fetch_page: F,
) -> Result<Vec<T>, QobuzError>
where
    T: Clone,
    F: FnMut(PageRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Page<T>, QobuzError>>,
{
    let mut all = Vec::new();
    let mut offset = 0u32;
    let limit = 500u32;
    let mut total = None;

    loop {
        let page = fetch_page(PageRequest { limit, offset }).await?;
        if total.is_none() {
            total = Some(page.total);
        }
        all.extend(page.items);
        offset = offset.saturating_add(page.limit);
        let total = total.unwrap_or(0);
        if offset >= total || page.limit == 0 {
            break;
        }
    }

    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_all_two_pages() {
        let pages = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let pages_clone = pages.clone();
        let items = fetch_all_pages(|req| {
            let n = pages_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let page = match n {
                0 => Page {
                    items: vec![1, 2],
                    total: 3,
                    limit: 2,
                    offset: req.offset,
                },
                _ => Page {
                    items: vec![3],
                    total: 3,
                    limit: 2,
                    offset: req.offset,
                },
            };
            async move { Ok(page) }
        })
        .await
        .unwrap();
        assert_eq!(items, vec![1, 2, 3]);
    }
}
