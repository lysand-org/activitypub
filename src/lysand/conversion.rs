use activitypub_federation::{fetch::object_id::ObjectId, http_signatures::generate_actor_keypair};
use activitystreams_kinds::public;
use anyhow::{anyhow, Ok};
use async_recursion::async_recursion;
use chrono::{DateTime, TimeZone, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use time::OffsetDateTime;
use url::Url;

use crate::{
    database::State,
    entities::{self, post, prelude, user},
    objects::post::Mention,
    utils::{generate_lysand_post_url, generate_object_id, generate_user_id},
    API_DOMAIN, DB, FEDERATION_CONFIG, LYSAND_DOMAIN,
};

use super::{
    objects::{CategoryType, ContentEntry, ContentFormat, Note, PublicKey},
    superx::request_client,
};

pub async fn fetch_user_from_url(url: Url) -> anyhow::Result<super::objects::User> {
    let req_client = request_client();
    let request = req_client.get(url).send().await?;
    Ok(request.json::<super::objects::User>().await?)
}

pub async fn lysand_post_from_db(
    post: entities::post::Model,
) -> anyhow::Result<super::objects::Note> {
    let data = FEDERATION_CONFIG.get().unwrap();
    let domain = data.domain();
    let url = generate_lysand_post_url(domain, &post.id)?;
    let author = Url::parse(&post.creator.to_string())?;
    let visibility = match post.visibility.as_str() {
        "public" => super::objects::VisibilityType::Public,
        "followers" => super::objects::VisibilityType::Followers,
        "direct" => super::objects::VisibilityType::Direct,
        "unlisted" => super::objects::VisibilityType::Unlisted,
        _ => super::objects::VisibilityType::Public,
    };
    //let mut mentions = Vec::new();
    //for obj in post.tag.clone() {
    //    mentions.push(obj.href.clone());
    //}
    let mut content = ContentFormat::default();
    content.x.insert(
        "text/html".to_string(),
        ContentEntry::from_string(post.content),
    );
    let note = super::objects::Note {
        rtype: super::objects::LysandType::Note,
        id: uuid::Uuid::parse_str(&post.id)?,
        author: author.clone(),
        uri: url.clone(),
        created_at: OffsetDateTime::from_unix_timestamp(post.created_at.timestamp()).unwrap(),
        content: Some(content),
        mentions: None,
        category: Some(CategoryType::Microblog),
        device: None,
        visibility: Some(visibility),
        previews: None,
        replies_to: None,
        quotes: None,
        group: None,
        attachments: None,
        subject: post.title,
        is_sensitive: Some(post.sensitive),
    };
    Ok(note)
}

pub async fn lysand_user_from_db(
    user: entities::user::Model,
) -> anyhow::Result<super::objects::User> {
    let url = Url::parse(&user.url)?;
    let inbox_url = Url::parse("https://ap.lysand.org/apbridge/lysand/inbox")?;
    let outbox_url = Url::parse(
        ("https://ap.lysand.org/apbridge/lysand/outbox/".to_string() + &user.id).as_str(),
    )?;
    let followers_url = Url::parse(
        ("https://ap.lysand.org/apbridge/lysand/followers/".to_string() + &user.id).as_str(),
    )?;
    let following_url = Url::parse(
        ("https://ap.lysand.org/apbridge/lysand/following/".to_string() + &user.id).as_str(),
    )?;
    let featured_url = Url::parse(
        ("https://ap.lysand.org/apbridge/lysand/featured/".to_string() + &user.id).as_str(),
    )?;
    let likes_url = Url::parse(
        ("https://ap.lysand.org/apbridge/lysand/likes/".to_string() + &user.id).as_str(),
    )?;
    let dislikes_url = Url::parse(
        ("https://ap.lysand.org/apbridge/lysand/dislikes/".to_string() + &user.id).as_str(),
    )?;
    let og_displayname_ref = user.name.clone();
    let og_username_ref = user.username.clone();
    let empty = "".to_owned();
    // linter was having a stroke
    let display_name = match og_displayname_ref {
        og_username_ref => None,
        empty => None,
        _ => Some(user.name),
    };
    let mut bio = ContentFormat::default();
    bio.x.insert(
        "text/html".to_string(),
        ContentEntry::from_string(user.summary.unwrap_or_default()),
    );
    let user = super::objects::User {
        rtype: super::objects::LysandType::User,
        id: uuid::Uuid::try_parse(&user.id)?,
        uri: url.clone(),
        username: user.username,
        display_name,
        inbox: inbox_url,
        outbox: outbox_url,
        followers: followers_url,
        following: following_url,
        featured: featured_url,
        likes: likes_url,
        dislikes: dislikes_url,
        bio: Some(bio),
        avatar: None,
        header: None,
        fields: None,
        indexable: false,
        created_at: OffsetDateTime::from_unix_timestamp(user.created_at.timestamp()).unwrap(),
        public_key: PublicKey {
            actor: url.clone(),
            public_key: "AAAAC3NzaC1lZDI1NTE5AAAAIMxsX+lEWkHZt9NOvn9yYFP0Z++186LY4b97C4mwj/f2"
                .to_string(), // dummy key
        },
    };
    Ok(user)
}

pub async fn option_content_format_text(opt: Option<ContentFormat>) -> Option<String> {
    if let Some(format) = opt {
        return Some(format.select_rich_text().await.unwrap());
    }

    None
}
#[async_recursion]
pub async fn db_post_from_url(url: Url) -> anyhow::Result<entities::post::Model> {
    if !url.domain().eq(&Some(LYSAND_DOMAIN.as_str())) {
        return Err(anyhow!("not lysands domain"));
    }
    let str_url = url.to_string();
    let post_res: Option<post::Model> = prelude::Post::find()
        .filter(entities::post::Column::Url.eq(str_url.clone()))
        .one(DB.get().unwrap())
        .await?;

    if let Some(post) = post_res {
        Ok(post)
    } else {
        let post = fetch_note_from_url(url.clone()).await?;
        let res = receive_lysand_note(post, "https://ap.lysand.org/example".to_string()).await?; // TODO: Replace user id with actual user id
        Ok(res)
    }
}

pub async fn db_user_from_url(url: Url) -> anyhow::Result<entities::user::Model> {
    if !url.domain().eq(&Some(LYSAND_DOMAIN.as_str()))
        && !url.domain().eq(&Some(API_DOMAIN.as_str()))
    {
        return Err(anyhow!("not lysands domain"));
    }
    let user_res: Option<user::Model> = prelude::User::find()
        .filter(entities::user::Column::Url.eq(url.to_string()))
        .one(DB.get().unwrap())
        .await?;

    if let Some(user) = user_res {
        Ok(user)
    } else {
        let ls_user = fetch_user_from_url(url).await?;
        let keypair = generate_actor_keypair()?;
        let user = entities::user::ActiveModel {
            id: Set(ls_user.id.to_string()),
            username: Set(ls_user.username.clone()),
            name: Set(ls_user.display_name.unwrap_or(ls_user.username)),
            inbox: Set(ls_user.inbox.to_string()),
            public_key: Set(keypair.public_key.clone()),
            private_key: Set(Some(keypair.private_key.clone())),
            last_refreshed_at: Set(Utc::now()),
            follower_count: Set(0),
            following_count: Set(0),
            url: Set(ls_user.uri.to_string()),
            local: Set(true),
            created_at: Set(
                DateTime::from_timestamp(ls_user.created_at.unix_timestamp(), 0).unwrap(),
            ),
            summary: Set(option_content_format_text(ls_user.bio).await),
            updated_at: Set(Some(Utc::now())),
            followers: Set(Some(ls_user.followers.to_string())),
            following: Set(Some(ls_user.following.to_string())),
            ..Default::default()
        };
        let db = DB.get().unwrap();
        Ok(user.insert(db).await?)
    }
}

pub async fn fetch_note_from_url(url: Url) -> anyhow::Result<super::objects::Note> {
    let req_client = request_client();
    let request = req_client.get(url).send().await?;
    Ok(request.json::<super::objects::Note>().await?)
}
#[async_recursion]
pub async fn receive_lysand_note(
    note: Note,
    db_id: String,
) -> anyhow::Result<entities::post::Model> {
    let lysand_author: entities::user::Model = db_user_from_url(note.author.clone()).await?;
    let user_res = prelude::User::find_by_id(db_id)
        .one(DB.get().unwrap())
        .await;
    if user_res.is_err() {
        println!("{}", user_res.as_ref().unwrap_err());
        return Err(user_res.err().unwrap().into());
    }
    if let Some(target) = user_res? {
        let data = FEDERATION_CONFIG.get().unwrap();
        let id: ObjectId<post::Model> =
            generate_object_id(data.domain(), &note.id.to_string())?.into();
        let user_id = generate_user_id(data.domain(), &target.id.to_string())?;
        let user = fetch_user_from_url(note.author.clone()).await?;
        let mut tag: Vec<Mention> = Vec::new();
        for l_tag in note.mentions.clone().unwrap_or_default() {
            tag.push(Mention {
                href: l_tag, //TODO convert to ap url
                kind: Default::default(),
            })
        }
        let mut mentions = Vec::new();
        for obj in tag.clone() {
            mentions.push(obj.href.clone());
        }
        let to = match note
            .visibility
            .clone()
            .unwrap_or(super::objects::VisibilityType::Public)
        {
            super::objects::VisibilityType::Public => {
                let mut vec = vec![public(), Url::parse(&user.followers.to_string().as_str())?];
                vec.append(&mut mentions.clone());
                vec
            }
            super::objects::VisibilityType::Followers => {
                let mut vec = vec![Url::parse(&user.followers.to_string().as_str())?];
                vec.append(&mut mentions.clone());
                vec
            }
            super::objects::VisibilityType::Direct => mentions.clone(),
            super::objects::VisibilityType::Unlisted => {
                let mut vec = vec![Url::parse(&user.followers.to_string().as_str())?];
                vec.append(&mut mentions.clone());
                vec
            }
        };
        let cc = match note
            .visibility
            .clone()
            .unwrap_or(super::objects::VisibilityType::Public)
        {
            super::objects::VisibilityType::Unlisted => Some(vec![public()]),
            _ => None,
        };
        let reply: Option<ObjectId<entities::post::Model>> =
            if let Some(rep) = note.replies_to.clone() {
                let note = fetch_note_from_url(rep).await?;
                let fake_rep_url = Url::parse(&format!(
                    "https://{}/apbridge/object/{}",
                    API_DOMAIN.to_string(),
                    &note.id.to_string()
                ))?;
                Some(fake_rep_url.into())
            } else {
                None
            };
        let quote: Option<ObjectId<entities::post::Model>> = if let Some(rep) = note.quotes.clone()
        {
            let note = fetch_note_from_url(rep).await?;
            let fake_rep_url = Url::parse(&format!(
                "https://{}/apbridge/object/{}",
                API_DOMAIN.to_string(),
                &note.id.to_string()
            ))?;
            Some(fake_rep_url.into())
        } else {
            None
        };
        let reply_uuid: Option<String> = if let Some(rep) = note.replies_to.clone() {
            Some(db_post_from_url(rep).await?.id)
        } else {
            None
        };
        let quote_uuid: Option<String> = if let Some(rep) = note.quotes.clone() {
            Some(db_post_from_url(rep).await?.id)
        } else {
            None
        };
        let ap_note = crate::objects::post::Note {
            kind: Default::default(),
            id,
            sensitive: Some(note.is_sensitive.unwrap_or(false)),
            cc,
            to,
            tag,
            attributed_to: Url::parse(user.uri.clone().as_str()).unwrap().into(),
            content: option_content_format_text(note.content)
                .await
                .unwrap_or_default(),
            in_reply_to: reply.clone(),
        };

        let visibility = match note
            .visibility
            .clone()
            .unwrap_or(super::objects::VisibilityType::Public)
        {
            super::objects::VisibilityType::Public => "public",
            super::objects::VisibilityType::Followers => "followers",
            super::objects::VisibilityType::Direct => "direct",
            super::objects::VisibilityType::Unlisted => "unlisted",
        };
        if let Some(obj) = note.replies_to {
            println!("Quoting: {}", db_post_from_url(obj).await?.url);
        }
        if let Some(obj) = note.quotes {
            println!("Replying to: {}", db_post_from_url(obj).await?.url);
        }
        let post = entities::post::ActiveModel {
            id: Set(note.id.to_string()),
            creator: Set(lysand_author.id.clone()),
            content: Set(ap_note.content.clone()),
            sensitive: Set(ap_note.sensitive.unwrap_or_default()),
            created_at: Set(Utc
                .timestamp_micros(note.created_at.unix_timestamp())
                .unwrap()),
            local: Set(true),
            updated_at: Set(Some(Utc::now())),
            content_type: Set("Note".to_string()),
            visibility: Set(visibility.to_string()),
            title: Set(note.subject.clone()),
            url: Set(note.uri.clone().to_string()),
            reply_id: Set(reply_uuid),
            quoting_id: Set(quote_uuid),
            spoiler_text: Set(note.subject),
            ap_json: Set(Some(serde_json::to_string(&ap_note).unwrap())),
            ..Default::default()
        };
        let res = post.insert(DB.get().unwrap()).await?;
        Ok(res)
    } else {
        Err(anyhow!("User not found"))
    }
}
