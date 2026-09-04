use crate::codec::{self, rfc3339, rfc3339_opt};
use crate::modules::desk::models::{
    AdminOverview, BannedLink, Gift, GiftCategory, NospamHit, NospamResult, Report, Ticket,
    TicketReply, UserGift, Voucher, Warning,
};
use crate::pb;

pub fn gift_to_pb(gift: &Gift) -> pb::Gift {
    pb::Gift {
        id: gift.id,
        category_id: gift.category_id,
        name: gift.name.clone(),
        description: gift.description.clone(),
        price: gift.price,
        image_url: gift.image_url.clone(),
    }
}

pub fn catalog_to_pb(categories: Vec<GiftCategory>) -> pb::GiftCatalog {
    pb::GiftCatalog {
        categories: categories
            .into_iter()
            .map(|category| pb::GiftCategory {
                id: category.id,
                slug: category.slug,
                name: category.name,
                description: category.description,
                gifts: category.gifts.iter().map(gift_to_pb).collect(),
            })
            .collect(),
    }
}

pub fn user_gift_to_pb(gift: &UserGift) -> pb::UserGift {
    pb::UserGift {
        id: gift.id,
        gift_id: gift.gift_id,
        gift: Some(gift_to_pb(&gift.gift)),
        sender_id: gift.sender_id,
        sender: gift.sender.as_ref().map(codec::user_to_pb),
        receiver_id: gift.receiver_id,
        caption: gift.caption.clone(),
        anonymous: gift.anonymous,
        created_at: rfc3339(gift.created_at),
    }
}

pub fn user_gifts_to_pb(gifts: Vec<UserGift>) -> pb::UserGiftList {
    pb::UserGiftList {
        gifts: gifts.iter().map(user_gift_to_pb).collect(),
    }
}

pub fn ticket_to_pb(ticket: &Ticket) -> pb::Ticket {
    pb::Ticket {
        id: ticket.id,
        author_id: ticket.author_id,
        author: Some(codec::user_to_pb(&ticket.author)),
        subject: ticket.subject.clone(),
        content: ticket.content.clone(),
        status: ticket.status.clone(),
        created_at: rfc3339(ticket.created_at),
        replies: ticket.replies.iter().map(reply_to_pb).collect(),
    }
}

fn reply_to_pb(reply: &TicketReply) -> pb::TicketReply {
    pb::TicketReply {
        id: reply.id,
        ticket_id: reply.ticket_id,
        author_id: reply.author_id,
        author: Some(codec::user_to_pb(&reply.author)),
        content: reply.content.clone(),
        from_agent: reply.from_agent,
        created_at: rfc3339(reply.created_at),
    }
}

pub fn tickets_to_pb(tickets: Vec<Ticket>) -> pb::TicketList {
    pb::TicketList {
        tickets: tickets.iter().map(ticket_to_pb).collect(),
    }
}

pub fn report_to_pb(report: &Report) -> pb::Report {
    pb::Report {
        id: report.id,
        author_id: report.author_id,
        author: Some(codec::user_to_pb(&report.author)),
        target_type: report.target_type.clone(),
        target_id: report.target_id,
        reason: report.reason.clone(),
        status: report.status.clone(),
        created_at: rfc3339(report.created_at),
    }
}

pub fn reports_to_pb(reports: Vec<Report>) -> pb::ReportList {
    pb::ReportList {
        reports: reports.iter().map(report_to_pb).collect(),
    }
}

pub fn voucher_to_pb(voucher: &Voucher) -> pb::Voucher {
    pb::Voucher {
        id: voucher.id,
        serial: voucher.serial.clone(),
        coins: voucher.coins,
        remaining: voucher.remaining,
        total: voucher.total,
        expires_at: rfc3339_opt(voucher.expires_at),
    }
}

pub fn vouchers_to_pb(vouchers: Vec<Voucher>) -> pb::VoucherList {
    pb::VoucherList {
        vouchers: vouchers.iter().map(voucher_to_pb).collect(),
    }
}

pub fn banned_link_to_pb(link: &BannedLink) -> pb::BannedLink {
    pb::BannedLink {
        id: link.id,
        url: link.url.clone(),
        reason: link.reason.clone(),
        created_at: rfc3339(link.created_at),
    }
}

pub fn banned_links_to_pb(links: Vec<BannedLink>) -> pb::BannedLinkList {
    pb::BannedLinkList {
        links: links.iter().map(banned_link_to_pb).collect(),
    }
}

pub fn warning_to_pb(warning: &Warning) -> pb::Warning {
    pb::Warning {
        id: warning.id,
        user_id: warning.user_id,
        actor_id: warning.actor_id,
        reason: warning.reason.clone(),
        created_at: rfc3339(warning.created_at),
    }
}

pub fn warnings_to_pb(warnings: Vec<Warning>) -> pb::WarningList {
    pb::WarningList {
        warnings: warnings.iter().map(warning_to_pb).collect(),
    }
}

pub fn nospam_to_pb(result: &NospamResult) -> pb::NospamResult {
    pb::NospamResult {
        action_id: result.action_id,
        hits: result.hits.iter().map(hit_to_pb).collect(),
        deleted: result.deleted,
    }
}

fn hit_to_pb(hit: &NospamHit) -> pb::NospamHit {
    pb::NospamHit {
        post_id: hit.post_id,
        target_id: hit.target_id,
        local_id: hit.local_id,
        author_id: hit.author_id,
        content: hit.content.clone(),
        permalink: hit.permalink.clone(),
    }
}

pub fn overview_to_pb(overview: &AdminOverview) -> pb::AdminOverview {
    pb::AdminOverview {
        users: overview.users,
        groups: overview.groups,
        wall_posts: overview.wall_posts,
        tickets_open: overview.tickets_open,
        reports_open: overview.reports_open,
        banned_users: overview.banned_users,
    }
}
